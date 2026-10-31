package rs.compose.android

import android.content.ClipData
import android.content.ClipboardManager
import android.content.Context
import android.content.Intent
import android.content.pm.ApplicationInfo
import android.content.res.Configuration
import android.text.format.DateFormat
import android.graphics.Canvas
import android.graphics.Rect
import android.graphics.Matrix
import android.graphics.RectF
import android.net.Uri
import android.os.Build
import android.os.Bundle
import android.os.Handler
import android.os.Looper
import android.os.Parcelable
import android.os.SystemClock
import android.os.Trace
import android.text.TextUtils
import android.util.Log
import android.view.Choreographer
import android.view.HapticFeedbackConstants
import android.view.KeyEvent
import android.view.MotionEvent
import android.view.View
import android.view.ViewConfiguration
import android.view.ViewTreeObserver
import android.view.accessibility.AccessibilityManager
import android.view.accessibility.AccessibilityNodeProvider
import android.view.inputmethod.CompletionInfo
import android.view.inputmethod.CorrectionInfo
import android.view.inputmethod.CursorAnchorInfo
import android.view.inputmethod.EditorBoundsInfo
import android.view.inputmethod.EditorInfo
import android.view.inputmethod.ExtractedText
import android.view.inputmethod.ExtractedTextRequest
import android.view.inputmethod.InputConnection
import android.view.inputmethod.InputContentInfo
import java.lang.ref.ReferenceQueue
import java.lang.ref.WeakReference
import android.view.inputmethod.InputMethodManager
import java.util.concurrent.TimeUnit

/**
 * A view showing a compose-rs app (`compose_android::export_apps!` names them), as a plain
 * `View`: in a Kotlin Compose UI through `AndroidView`, it's transformed, clipped and scrolled as
 * any content (docs/07-platforms.md, "Embedding").
 *
 * On a Choreographer frame (asked for when the app's state changes), Rust recomposes, lays out
 * and records a display list; [onDraw] replays it on the view's (hardware) canvas. After a frame,
 * until the next one should start, lazy lists that scrolled compose and measure the items they'll
 * reach next (prefetch, as Compose's `AndroidPrefetchScheduler`). While the window is hidden, no
 * frame is asked for: the one missed is made when it shows. The composition lives while the
 * view is attached to a window.
 *
 * What the content's `remember_saveable`s hold survives the composition: kept when the view
 * detaches (and given back when it attaches again), and saved in the view's instance state (an
 * activity recreated for a configuration change, or after the process was killed), which needs
 * the view to have an id: it takes `R.id.compose_rs_view` unless given another (two views in one
 * window need ids of their own). Back goes to the content's `BackHandler`s while one is enabled.
 * The app receives properties ([setProp]) and sends events ([setOnEventListener]).
 *
 * It draws as Compose does (its graphics layers, clips and shadows as Compose's render nodes),
 * with compose-rs's own optimizations ([optimizations]): by default the recommended set
 * ([ComposeRsOptimizations.RECOMMENDED], D610), [ComposeRsOptimizations.NONE] for exactly
 * Compose's rendering. One of them puts, while it shows on API 33+, the RenderThread and the main
 * thread in a performance hint session of compose-rs's own, so Android boosts them by the frames'
 * own work, not only after long main-thread frames ([isPerformanceHintEnabled]).
 */
class ComposeRsView @JvmOverloads constructor(
    context: Context,
    private val app: String,
    optimizations: Long = ComposeRsOptimizations.DEFAULT,
) : View(context) {
    /**
     * The host's paragraphs by handle, a new one for each host: a `TextMeasurer` or a result its
     * content kept outlives it, still laying out and releasing, and each host numbers from 0
     * (D1118).
     */
    private var measurer = newMeasurer()
    private val replayer = Replayer(measurer)
    /** Over the Rust host's drawing (see [ComposeRsNative.takeDrawing]). */
    private var drawing: java.nio.ByteBuffer? = null
    private var host = 0L
    private var frameRequested = false
    /** A frame was asked for while the window was hidden: made when it shows ([requestFrame]). */
    private var frameWanted = false
    private var textInput = false
    private val collectedConnections = ReferenceQueue<ComposeRsInputConnection>()
    private val openConnections = HashSet<ConnectionReference>()
    internal val nativeHost: Long get() = host
    private val inputMethods = context.getSystemService(InputMethodManager::class.java)

    /**
     * Whether, while this view shows (API 33+), the RenderThread and the main thread are in
     * compose-rs's own performance hint session, which reports both threads' CPU time over each
     * frame against a target, so Android boosts them by the frames' work (on by default, in the
     * recommended set; Compose has no such session, so parity runs turn it off: D390, D440,
     * D500, D610).
     * Android boosts a thread through HWUI's own session too, but only after long frames
     * counting the main thread's, which compose-rs's are rarely: without it, the RenderThread
     * mostly runs on slower cores at lower frequencies, and its frames take longer for the same
     * work. Off, only HWUI's session boosts it, which costs less power. Set from another thread,
     * it applies on the main thread.
     */
    var isPerformanceHintEnabled: Boolean
        get() = optimizations and ComposeRsOptimizations.PERFORMANCE_HINT != 0L
        set(value) {
            if (Looper.myLooper() != Looper.getMainLooper()) {
                mainHandler.post { isPerformanceHintEnabled = value }
                return
            }
            optimizations = if (value) {
                optimizations or ComposeRsOptimizations.PERFORMANCE_HINT
            } else {
                optimizations and ComposeRsOptimizations.PERFORMANCE_HINT.inv()
            }
        }

    /**
     * compose-rs's own optimizations beyond Compose's rendering, as [ComposeRsOptimizations]'
     * bits ([ComposeRsOptimizations.DEFAULT], the recommended set, unless given at construction:
     * D610). [ComposeRsOptimizations.NONE] does the work Compose does, which the benchmarks pass
     * so a run against Compose measures the same rendering, and each optimization apart (D520).
     * Set from another thread, it applies on the main thread (read from any); a change to one
     * that draws draws everything again.
     */
    @Volatile
    var optimizations: Long = optimizations
        set(value) {
            if (Looper.myLooper() != Looper.getMainLooper()) {
                mainHandler.post { this.optimizations = value }
                return
            }
            if (value == field) return
            field = value
            applyOptimizations(value)
            if (host != 0L) {
                ComposeRsNative.setOptimizations(host, value)
                requestFrame()
            }
            updatePerformanceHint()
        }

    init {
        applyOptimizations(optimizations)
    }

    private fun newMeasurer() =
        TextMeasurer(
            checkBoring = context.applicationInfo.flags and ApplicationInfo.FLAG_DEBUGGABLE != 0,
            assets = context.assets,
        )

    /** What the Kotlin side does by [optimizations]: paragraphs' render nodes, the flush layer. */
    private fun applyOptimizations(value: Long) {
        measurer.paragraphNodes = value and ComposeRsOptimizations.PARAGRAPH_RENDER_NODES != 0L
        replayer.optimizations = value
    }

    /** The host was told it shows with hints enabled ([ComposeRsNative.setPerformanceHint]). */
    private var performanceHintShown = false

    private val frameCallback = Choreographer.FrameCallback { time ->
        noteFrameTime(time)
        frame()
    }

    /**
     * The last Choreographer callback's frame time and when it started (`System.nanoTime`):
     * where the next frame starts comes from both ([ComposeRsNative.nextFrameNanos]), as the
     * frame time may lag its vsync by an interval (buffer stuffing recovery: D340).
     */
    private var frameTimeNanos = 0L
    private var frameStartedNanos = 0L
    private var prefetchScheduled = false
    private val prefetchRunnable = Runnable { prefetch() }

    /** Idle times in a row that had no time before the next frame (see [prefetch]). */
    private var idleTimesWithoutTime = 0

    /** How long drawing an item ahead and recording its groups takes ([recordAhead]). */
    private val drawAheadCost = StepCost()

    /**
     * The frame time of the last [prefetchFrameCallback]: Compose's `AndroidPrefetchScheduler`'s
     * `frameStartTimeNanos`, the frame callbacks it posts itself (not the frames').
     */
    private var prefetchFrameTimeNanos = 0L

    /** Prefetch work left: tried again after the next frame. */
    private val prefetchFrameCallback = Choreographer.FrameCallback { time ->
        noteFrameTime(time)
        prefetchFrameTimeNanos = time
        post(prefetchRunnable)
    }

    private fun noteFrameTime(time: Long) {
        frameTimeNanos = time
        frameStartedNanos = System.nanoTime()
    }
    private val accessibility = Accessibility(this)
    private val accessibilityManager = context.getSystemService(AccessibilityManager::class.java)

    /** What the last composition saved (or the instance state restored), for the next one. */
    private var savedState: ByteArray? = null

    /** Back, intercepted while the content has an enabled `BackHandler`. */
    private var back: BackRegistration? = null
    private var backEnabled = false

    /** The window's insets, as the content reads them (see [ComposeRsInsets]). */
    private val insets = ComposeRsInsets(
        this,
        send = { values, animating ->
            if (host != 0L) {
                ComposeRsNative.setWindowInsets(host, values)
                frameBeforeDraw(animating)
            }
        },
        sendCutoutPath = { points ->
            if (host != 0L) {
                ComposeRsNative.setWindowCutoutPath(host, points)
                frameBeforeDraw(animating = false)
            }
        },
    )

    /**
     * Whether the view returns the window's insets consumed to whatever dispatched them, as
     * Compose's `AbstractComposeView.consumeWindowInsets`: its later siblings don't get them, and an
     * animation of them stops here. It has no views below it. False by default.
     *
     * A view pads by the insets it's given. Inside Kotlin Compose's `AndroidView` those are reduced
     * by its distance from the window's edges; under a plain parent that pads itself without
     * consuming them (`setPadding` in a listener returning the insets), they aren't, so its content
     * pads a second time: consume them in the parent.
     */
    var consumeWindowInsets: Boolean
        get() = insets.consumes
        set(value) {
            insets.consumes = value
        }

    /** The properties given to the content, set again on each composition. */
    private val props = LinkedHashMap<String, Any>()
    private var eventListener: OnEventListener? = null
    private val mainHandler = Handler(Looper.getMainLooper())

    /** Receives the events the content sends (`LOCAL_HOST_EVENTS`). */
    fun interface OnEventListener {
        fun onEvent(name: String, payload: String)
    }

    init {
        isFocusable = true
        isFocusableInTouchMode = true
        // The content shows its own focus.
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.O) defaultFocusHighlightEnabled = false
        // Its instance state is saved only with an id.
        if (id == NO_ID) id = R.id.compose_rs_view
        insets.attach()
    }

    private var lifecycleSource: LifecycleSource? = null

    /** The input mode is the view system's, as `AndroidComposeView`'s. */
    private val touchModeChangeListener =
        ViewTreeObserver.OnTouchModeChangeListener { touchMode ->
            if (host != 0L) ComposeRsNative.setInputMode(host, touchMode)
        }

    /**
     * As `AndroidComposeView.requestFocus`: the view system moving the focus here (a navigation
     * key leaving touch mode, a focus search, the window's first focus) gives it to the content,
     * which says if it can take it.
     */
    override fun requestFocus(direction: Int, previouslyFocusedRect: Rect?): Boolean {
        if (isFocused) return true
        if (host == 0L) return super.requestFocus(direction, previouslyFocusedRect)
        val rect = previouslyFocusedRect
        val taken = ComposeRsNative.ownerRequestedFocus(
            host,
            direction,
            rect != null,
            rect?.left?.toFloat() ?: 0f,
            rect?.top?.toFloat() ?: 0f,
            rect?.right?.toFloat() ?: 0f,
            rect?.bottom?.toFloat() ?: 0f,
        )
        return taken && super.requestFocus(direction, previouslyFocusedRect)
    }

    override fun onFocusChanged(gainFocus: Boolean, direction: Int, previouslyFocusedRect: Rect?) {
        super.onFocusChanged(gainFocus, direction, previouslyFocusedRect)
        if (!gainFocus && !hasFocus() && host != 0L) ComposeRsNative.ownerLostFocus(host)
    }

    override fun onAttachedToWindow() {
        super.onAttachedToWindow()
        viewTreeObserver.addOnTouchModeChangeListener(touchModeChangeListener)
        ComposeRsRuntime.attach(this)
        accessibility.attach()
        insets.resetState()
        requestApplyInsets()
        if (width > 0 && height > 0) create()
    }

    override fun onDetachedFromWindow() {
        removeCallbacks(sendHoverExit)
        hoverExitPending = false
        viewTreeObserver.removeOnTouchModeChangeListener(touchModeChangeListener)
        ComposeRsRuntime.detach(this)
        destroy()
        accessibility.detach()
        back?.remove()
        back = null
        backEnabled = false
        super.onDetachedFromWindow()
    }

    override fun onSizeChanged(width: Int, height: Int, oldWidth: Int, oldHeight: Int) {
        super.onSizeChanged(width, height, oldWidth, oldHeight)
        if (host == 0L) {
            if (isAttachedToWindow && width > 0 && height > 0) create()
        } else {
            ComposeRsNative.resize(host, width, height)
            requestFrame()
        }
    }

    private fun create() {
        if (host != 0L) return
        val metrics = resources.displayMetrics
        val lifecycle = LifecycleSource.of(this) { state ->
            if (host != 0L) ComposeRsNative.setLifecycleState(host, state)
        }
        lifecycleSource = lifecycle
        host = ComposeRsNative.create(
            this,
            measurer,
            measurer.results,
            app,
            width,
            height,
            metrics.density,
            resources.configuration.fontScale,
            layoutDirection == LAYOUT_DIRECTION_RTL,
            savedState,
            optimizations,
            lifecycle.state,
            HostViewModels.tokenOf(this),
        )
        savedState = null
        ComposeRsNative.setInputMode(host, isInTouchMode)
        sendConfiguration(resources.configuration)
        ComposeRsNative.setWindowInsets(host, insets.values)
        insets.cutoutPath?.let { ComposeRsNative.setWindowCutoutPath(host, it) }
        for ((name, value) in props) sendProp(name, value)
        updatePerformanceHint()
        // Composed, laid out and recorded now (during the window's layout), so the first frame
        // the window draws shows the content, as a Compose view's does.
        frame()
    }

    private fun destroy() {
        if (host == 0L) return
        // Given back if it's composed again (attached again, or a new view of the same id).
        savedState = ComposeRsNative.saveState(host)
        destroyHost()
    }

    /** Ends [host] and all that goes with it. */
    private fun destroyHost() {
        lifecycleSource?.remove()
        lifecycleSource = null
        cancelFrames()
        // (Destroying a host shown with hints hides it.)
        performanceHintShown = false
        ComposeRsNative.destroy(host)
        host = 0L
        // Its nodes are gone (a host composed again has others).
        accessibility.semanticsChanged(false)
        drawing = null
        // What it drew goes; its layouts stay with it, for what its content kept (D1118).
        measurer.clearDrawing()
        val paragraphNodes = measurer.paragraphNodes
        measurer = newMeasurer()
        measurer.paragraphNodes = paragraphNodes
        replayer.text = measurer
        replayer.clear()
        if (textInput) {
            textInput = false
            inputMethods.hideSoftInputFromWindow(windowToken, 0)
        }
    }

    override fun onConfigurationChanged(configuration: Configuration) {
        super.onConfigurationChanged(configuration)
        if (host != 0L) {
            ComposeRsNative.setDensity(host, resources.displayMetrics.density, configuration.fontScale)
            sendConfiguration(configuration)
            requestFrame()
        }
    }

    private fun sendConfiguration(configuration: Configuration) {
        val night = configuration.uiMode and Configuration.UI_MODE_NIGHT_MASK == Configuration.UI_MODE_NIGHT_YES
        ComposeRsNative.setConfiguration(
            host,
            configuration.screenWidthDp,
            configuration.screenHeightDp,
            configuration.orientation == Configuration.ORIENTATION_LANDSCAPE,
            configuration.fontScale,
            resources.displayMetrics.density,
            night,
            DateFormat.is24HourFormat(context),
        )
    }

    // ---- Saved state ---------------------------------------------------------------------

    override fun onSaveInstanceState(): Parcelable {
        val bundle = Bundle()
        bundle.putParcelable(SUPER_STATE, super.onSaveInstanceState())
        val state = if (host != 0L) ComposeRsNative.saveState(host) else savedState
        if (state != null) bundle.putByteArray(COMPOSE_RS_STATE, state)
        return bundle
    }

    override fun onRestoreInstanceState(state: Parcelable?) {
        if (state !is Bundle) {
            super.onRestoreInstanceState(state)
            return
        }
        @Suppress("DEPRECATION")
        super.onRestoreInstanceState(state.getParcelable(SUPER_STATE))
        val restored = state.getByteArray(COMPOSE_RS_STATE) ?: return
        savedState = restored
        // Already composed (restored late): composed again from the saved state.
        if (host != 0L) {
            destroyHost()
            create()
            invalidate()
        }
    }

    // ---- Back ----------------------------------------------------------------------------

    private fun updateBack(enabled: Boolean) {
        if (enabled == backEnabled) return
        backEnabled = enabled
        val registration = back ?: BackRegistration.create(this, backGesture).also { back = it }
        registration.setEnabled(enabled)
    }

    private val backGesture = object : BackGesture {
        override fun onBack() = handleBack()

        override fun onBackStarted(
            swipeEdge: Int,
            progress: Float,
            touchX: Float,
            touchY: Float,
            frameTimeMillis: Long,
        ) {
            if (host == 0L) return
            ComposeRsNative.onBackStarted(host, swipeEdge, progress, touchX, touchY, frameTimeMillis)
            ComposeRsRuntime.schedule(ComposeRsNative.nextTimer())
        }

        override fun onBackProgressed(
            swipeEdge: Int,
            progress: Float,
            touchX: Float,
            touchY: Float,
            frameTimeMillis: Long,
        ) {
            if (host == 0L) return
            ComposeRsNative.onBackProgressed(host, swipeEdge, progress, touchX, touchY, frameTimeMillis)
            ComposeRsRuntime.schedule(ComposeRsNative.nextTimer())
        }

        override fun onBackCancelled() {
            if (host == 0L) return
            ComposeRsNative.onBackCancelled(host)
            ComposeRsRuntime.schedule(ComposeRsNative.nextTimer())
        }
    }

    private fun handleBack() {
        if (host != 0L && ComposeRsNative.onBackPressed(host)) {
            ComposeRsRuntime.schedule(ComposeRsNative.nextTimer())
            return
        }
        // The handler went in the meantime: back goes on, to the activity.
        backEnabled = false
        back?.passOn()
    }

    // ---- Insets, properties and events ----------------------------------------------------

    /**
     * Sets the property [name] for the content (`LOCAL_HOST_PROPS`), recomposing its readers.
     * Any thread: from another, it's posted to the main thread.
     */
    fun setProp(name: String, value: String) = putProp(name, value)

    fun setProp(name: String, value: Double) = putProp(name, value)

    fun setProp(name: String, value: Int) = putProp(name, value.toDouble())

    fun setProp(name: String, value: Boolean) = putProp(name, value)

    /** Removes the property [name]. On the main thread (or posted to it). */
    fun removeProp(name: String) {
        if (Looper.myLooper() != Looper.getMainLooper()) {
            mainHandler.post { removeProp(name) }
            return
        }
        props.remove(name)
        if (host != 0L) ComposeRsNative.setProp(host, name, 0, null, 0.0)
    }

    /** A property set from another thread is posted to the main thread, where Rust runs. */
    private fun putProp(name: String, value: Any) {
        if (Looper.myLooper() != Looper.getMainLooper()) {
            mainHandler.post { putProp(name, value) }
            return
        }
        props[name] = value
        if (host != 0L) sendProp(name, value)
    }

    private fun sendProp(name: String, value: Any) {
        when (value) {
            is String -> ComposeRsNative.setProp(host, name, 1, value, 0.0)
            is Double -> ComposeRsNative.setProp(host, name, 2, null, value)
            is Boolean -> ComposeRsNative.setProp(host, name, 3, null, if (value) 1.0 else 0.0)
        }
    }

    /** Receives the events the content sends (`LOCAL_HOST_EVENTS.current().send(name, payload)`). */
    fun setOnEventListener(listener: OnEventListener?) {
        eventListener = listener
    }

    /**
     * An event from the content (Rust calls it, in the middle of a call into Rust): delivered
     * from the main looper once that call returned, so the listener may call into the view
     * (set a property, detach it) without re-entering Rust.
     */
    fun dispatchHostEvent(name: String, payload: String) {
        mainHandler.post { eventListener?.onEvent(name, payload) }
    }

    /**
     * Asks for a frame (Rust calls it when the app's state changes). While the window is hidden
     * (the activity stopped, the screen off), none: the frame is noted, and made when it shows
     * again ([onWindowVisibilityChanged]), as Compose pauses its frame clock when stopped (D342).
     */
    fun requestFrame() {
        if (frameRequested || host == 0L) return
        if (windowVisibility != VISIBLE) {
            frameWanted = true
            return
        }
        frameRequested = true
        Choreographer.getInstance().postFrameCallback(frameCallback)
    }

    /**
     * Hidden, the window asks for no frame: an animation would otherwise run a frame on every
     * vsync, drawing nothing. Shown again, the frame it missed (state changed, animations) is
     * made in the traversal that shows the window, after its layout and before it draws, so it
     * shows the content as of now; animations go on from the frame's time, as their clock is
     * the time itself.
     */
    override fun onWindowVisibilityChanged(visibility: Int) {
        super.onWindowVisibilityChanged(visibility)
        if (host == 0L) return
        updatePerformanceHint(visibility)
        if (visibility == VISIBLE) {
            if (frameWanted) {
                frameWanted = false
                // Asked for as any frame, and made sooner, before the traversal showing the
                // window draws, if one does (it dispatched this before its layout: a size changed
                // while hidden is applied first).
                requestFrame()
                if (!showFramePending) {
                    showFramePending = true
                    viewTreeObserver.addOnPreDrawListener(showFrame)
                }
            }
            // Prefetch work left when the window hid.
            if (prefetchLeft) {
                prefetchLeft = false
                schedulePrefetch()
            }
        } else {
            if (frameRequested) {
                Choreographer.getInstance().removeFrameCallback(frameCallback)
                frameRequested = false
                frameWanted = true
            }
            // A hidden window prefetches nothing (prefetch() checks, too).
            Choreographer.getInstance().removeFrameCallback(prefetchFrameCallback)
            removeCallbacks(prefetchRunnable)
            if (prefetchScheduled) prefetchLeft = true
            prefetchScheduled = false
            idleTimesWithoutTime = 0
        }
    }

    /**
     * Tells the host whether it shows with performance hints enabled: a host, a visible window
     * ([visibility]: the window's), API 33+ (`APerformanceHint`). The hint session is the
     * process's, open while one does.
     */
    private fun updatePerformanceHint(visibility: Int = windowVisibility) {
        val shown = host != 0L && isPerformanceHintEnabled && visibility == VISIBLE &&
            Build.VERSION.SDK_INT >= 33
        if (shown == performanceHintShown) return
        performanceHintShown = shown
        ComposeRsNative.setPerformanceHint(host, shown)
    }

    /** The frame missed while hidden, made before the traversal that shows the window draws. */
    private var showFramePending = false

    /** Prefetch work was left when the window hid: scheduled again when it shows. */
    private var prefetchLeft = false

    /**
     * The insets changed: if what read them asked for a frame, it's made before this vsync's
     * traversal draws, not at the next vsync. They arrive in a traversal (dispatched before its
     * layout; the first time, in an `AndroidView`, reduced during its layout, after [create] made
     * the first frame with them whole, D1365) or, [animating], in the vsync's insets animation step,
     * after the frame callbacks, with no traversal unless it's asked for. A view whose content reads
     * none has no frame requested: nothing happens.
     */
    private fun frameBeforeDraw(animating: Boolean) {
        if (!frameRequested) return
        if (!showFramePending) {
            showFramePending = true
            viewTreeObserver.addOnPreDrawListener(showFrame)
        }
        if (animating) invalidate()
    }

    private val showFrame = object : ViewTreeObserver.OnPreDrawListener {
        override fun onPreDraw(): Boolean {
            viewTreeObserver.removeOnPreDrawListener(this)
            showFramePending = false
            if (host != 0L && frameRequested) {
                Choreographer.getInstance().removeFrameCallback(frameCallback)
                frame()
            }
            return true
        }
    }

    /** No frame, prefetch or frame on show is waiting any more (the host goes). */
    private fun cancelFrames() {
        val choreographer = Choreographer.getInstance()
        choreographer.removeFrameCallback(frameCallback)
        frameRequested = false
        frameWanted = false
        choreographer.removeFrameCallback(prefetchFrameCallback)
        removeCallbacks(prefetchRunnable)
        prefetchScheduled = false
        prefetchLeft = false
        idleTimesWithoutTime = 0
        cancelShowFrame()
    }

    private fun cancelShowFrame() {
        if (showFramePending) {
            showFramePending = false
            viewTreeObserver.removeOnPreDrawListener(showFrame)
        }
    }

    private fun frame() {
        frameRequested = false
        // Any frame is the one missed while hidden (if the traversal showing the window ended
        // before drawing, the frame callback made it: its listener mustn't make the next one).
        cancelShowFrame()
        if (host == 0L) return
        val result = ComposeRsNative.frame(host)
        if (result and Frame.DREW != 0L) invalidate()
        if (result and Frame.AGAIN != 0L) requestFrame()
        if (result and Frame.PREFETCH != 0L) schedulePrefetch()
        updateBack(result and Frame.BACK != 0L)
        ComposeRsRuntime.schedule(Frame.nextTimer(result))
        updateTextInput(result and Frame.TEXT_INPUT != 0L, result and Frame.RESET_INPUT != 0L)
        if (result and Frame.INPUT_METHOD_UPDATE != 0L) updateInputMethod()
        // What the content asked of the keyboard (after the frame's events, which woke it).
        if (result and Frame.SHOW_KEYBOARD != 0L && textInput) {
            requestFocus()
            inputMethods.showSoftInput(this, 0)
        } else if (result and Frame.HIDE_KEYBOARD != 0L) {
            inputMethods.hideSoftInputFromWindow(windowToken, 0)
        }
        // The window's focus follows the content's, as `requestOwnerFocus`, `clearOwnerFocus`.
        if (result and Frame.OWNER_FOCUS != 0L) {
            if (!isFocused) requestFocus()
        } else if (result and Frame.OWNER_CLEAR != 0L) {
            if (isFocused) clearFocus()
        }
        // Read again when next asked, service or not (D630); the services on hear it.
        if (result and Frame.SEMANTICS != 0L) {
            accessibility.semanticsChanged(result and Frame.SEMANTICS_AT_ONCE != 0L)
        }
    }

    /** Prefetches once the frame is done (a message after the Choreographer's callbacks). */
    private fun schedulePrefetch() {
        if (prefetchScheduled) return
        prefetchScheduled = true
        post(prefetchRunnable)
    }

    private fun prefetch() {
        // A hidden window draws no frames to prepare for, as Compose's.
        if (host == 0L || windowVisibility != VISIBLE) {
            if (host != 0L) prefetchLeft = true
            prefetchScheduled = false
            idleTimesWithoutTime = 0
            return
        }
        val refreshRate = display?.refreshRate?.coerceAtLeast(30f) ?: 60f
        val interval = (1_000_000_000 / refreshRate).toLong()
        val ahead = optimizations and ComposeRsOptimizations.DRAW_PREFETCHED_AHEAD != 0L
        if (optimizations and ComposeRsOptimizations.PREFETCH_BUDGET_RECENT_MAXIMUM == 0L) {
            prefetchAsCompose(interval, ahead)
            return
        }
        // Until the next frame: the vsync after the last frame's, found from its frame time and
        // start, as a frame time shifted by buffer stuffing recovery would leave no time at all
        // (D340). The shifted time itself is the animations', not changed here.
        // (`display.refreshRate`: right after a change of rate, a frame time from before it may
        // put the next frame up to an interval late, once.)
        val nextFrameNanos = ComposeRsNative.nextFrameNanos(frameTimeNanos, interval, frameStartedNanos)
        if (nextFrameNanos - System.nanoTime() <= RECORD_MARGIN_NANOS) {
            // No time, or less than the margin, which no step starts in and which would only
            // lower Rust's estimates (the frame ran past the next vsync, or came from a layout,
            // not a callback): tried after the next frame, which gives a fresh time; a few times at
            // most, then the next frame with prefetch work schedules it again, rather than
            // asking for every vsync for good.
            if (++idleTimesWithoutTime > MOST_IDLE_TIMES_WITHOUT_TIME) {
                idleTimesWithoutTime = 0
                prefetchScheduled = false
            } else {
                Choreographer.getInstance().postFrameCallback(prefetchFrameCallback)
            }
            return
        }
        idleTimesWithoutTime = 0
        // Recording stops a margin before it, as Rust's prefetch steps do: its estimate is an
        // average, and a recording running past the vsync delays the frame (D341).
        val recordDeadline = nextFrameNanos - RECORD_MARGIN_NANOS
        // Paragraphs the last idle time laid out but had no time to record go first, then the
        // groups of the items it measured: their item is the next one shown.
        var workLeft = ahead && (
            measurer.recordLaidOut(recordDeadline) ||
                recordAhead(recordDeadline, firstInIdleTime = measurer.recordedLast == 0)
            )
        if (!workLeft) {
            val available = nextFrameNanos - System.nanoTime()
            workLeft = available <= RECORD_MARGIN_NANOS ||
                prefetchAndRecord(available, recordDeadline, unboundedFirst = false, ahead = ahead)
        }
        if (!workLeft) {
            prefetchScheduled = false
        } else {
            Choreographer.getInstance().postFrameCallback(prefetchFrameCallback)
        }
    }

    /**
     * Prefetches as Compose's `AndroidPrefetchScheduler.run`: until the later of the last drawn
     * frame's time (`drawingTime`) and the last frame callback it posted itself, plus an
     * interval; with no frame drawn for two intervals, the first request without a deadline
     * (`isFrameIdle`). Work left (a request whose next step didn't fit) goes on after the next
     * frame, each frame until it's done (D561). With [ahead], what it measured is recorded until
     * a margin before that deadline.
     */
    private fun prefetchAsCompose(interval: Long, ahead: Boolean) {
        idleTimesWithoutTime = 0
        val now = System.nanoTime()
        val drawn = TimeUnit.MILLISECONDS.toNanos(drawingTime)
        val idle = now > drawn + 2 * interval
        val nextFrameNanos = maxOf(prefetchFrameTimeNanos, drawn) + interval
        val available = (nextFrameNanos - now).coerceAtLeast(0L)
        var workLeft = ahead && (
            measurer.recordLaidOut(nextFrameNanos - RECORD_MARGIN_NANOS) ||
                recordAhead(nextFrameNanos - RECORD_MARGIN_NANOS, firstInIdleTime = measurer.recordedLast == 0)
            )
        if (!workLeft) {
            workLeft = if (available > 0 || idle) {
                prefetchAndRecord(available, nextFrameNanos - RECORD_MARGIN_NANOS, idle, ahead)
            } else {
                true
            }
        }
        if (!workLeft) {
            prefetchScheduled = false
        } else {
            Choreographer.getInstance().postFrameCallback(prefetchFrameCallback)
        }
    }

    /**
     * Prefetches for [available] nanoseconds ([unboundedFirst]: the first request without a
     * deadline); with [ahead], records the paragraphs it laid out and the groups of the items it
     * measured until [deadlineNanos], so the frame showing their item only draws them. Returns
     * whether work is left (prefetching, or paragraphs or groups to record).
     */
    private fun prefetchAndRecord(available: Long, deadlineNanos: Long, unboundedFirst: Boolean, ahead: Boolean): Boolean {
        measurer.collectingLaidOut = ahead
        val prefetchLeft = try {
            ComposeRsNative.prefetch(host, available, unboundedFirst)
        } finally {
            measurer.collectingLaidOut = false
        }
        if (!ahead) return prefetchLeft
        val recordingLeft = measurer.recordLaidOut(deadlineNanos) || recordAhead(deadlineNanos, firstInIdleTime = false)
        return prefetchLeft || recordingLeft
    }

    /**
     * Draws ahead the items a prefetch measured, oldest first, while one is expected to end
     * before [deadlineNanos] ([drawAheadCost], as Rust's prefetch steps: D341): Rust draws an
     * item's groups and writes their records, the replayer records their render nodes, so the
     * frame placing the item draws them by reference without recording (D350). [firstInIdleTime]: nothing
     * ran yet in this idle time. A step that doesn't fit after other work waits for the next idle
     * time; one that doesn't fit an idle time of its own gives up the items waiting (the frames
     * draw them), as Rust's prefetch gives up until the next scroll, rather than asking for every
     * vsync until a lowered estimate lets one start and overrun it. Returns whether some are
     * left, for the next idle time.
     */
    private fun recordAhead(deadlineNanos: Long, firstInIdleTime: Boolean): Boolean {
        if (!ComposeRsNative.hasDrawAhead(host)) return false
        Trace.beginSection("compose-rs: record ahead")
        try {
            var first = firstInIdleTime
            while (true) {
                val start = System.nanoTime()
                if (start + drawAheadCost.estimate() >= deadlineNanos) {
                    drawAheadCost.missed(first)
                    if (!first) return true
                    ComposeRsNative.cancelDrawAhead(host)
                    return false
                }
                first = false
                val taken = ComposeRsNative.drawAhead(host)
                val length = (taken and 0xFFFFFFFFL).toInt()
                if (length > 0) {
                    if (taken and ComposeRsNative.NEW_DRAWING_BUFFER != 0L || drawing == null) {
                        drawing = ComposeRsNative.drawingBuffer(host)
                    }
                    val buffer = drawing ?: return false
                    buffer.limit(length)
                    // Timed only when recorded: a step with nothing recorded is no sample.
                    if (replayer.recordAhead(buffer)) drawAheadCost.timed(System.nanoTime() - start)
                }
                if (taken and ComposeRsNative.MORE_AHEAD == 0L) return false
            }
        } finally {
            Trace.endSection()
        }
    }

    // ---- Accessibility -------------------------------------------------------------------

    override fun getAccessibilityNodeProvider(): AccessibilityNodeProvider = accessibility

    override fun dispatchHoverEvent(event: MotionEvent): Boolean {
        settleHoverExit(dropped = false)
        if (accessibilityManager.isTouchExplorationEnabled && accessibility.hover(event)) return true
        return super.dispatchHoverEvent(event)
    }

    internal fun semantics() = if (host != 0L) ComposeRsNative.semantics(host) else null

    internal fun semanticsNodeAt(x: Float, y: Float) =
        if (host != 0L) ComposeRsNative.nodeAt(host, x, y) else -1

    internal fun semanticsChanges(forget: Boolean) =
        if (host != 0L) ComposeRsNative.semanticsChanges(host, forget) else null

    internal fun performSemanticsAction(id: Int, action: Int, value: Float): Boolean {
        if (host == 0L) return false
        // An accessibility service moving the focus leaves touch mode, as Compose's
        // `AccessibilityTouchModeTest`: the system's, which tells the input mode.
        if (action == ACTION_REQUEST_FOCUS) requestKeyboardMode()
        return ComposeRsNative.performAction(host, id, action, value)
    }

    internal fun semanticsShape(id: Int, key: Int) =
        if (host != 0L) ComposeRsNative.semanticsShape(host, id, key) else null

    internal fun showSemanticsOnScreen(id: Int): Int =
        if (host != 0L) ComposeRsNative.showOnScreen(host, id) else 0

    internal fun semanticsRectangleOnScreen(id: Int): IntArray? =
        if (host != 0L) ComposeRsNative.rectangleOnScreen(host, id) else null

    internal fun characterLocations(id: Int, start: Int, length: Int): FloatArray? =
        if (host != 0L) ComposeRsNative.characterLocations(host, id, start, length) else null

    internal fun performTextAction(
        id: Int,
        action: Int,
        first: Int,
        second: Int,
        extendSelection: Boolean,
        text: String,
    ): Boolean =
        host != 0L &&
            ComposeRsNative.performTextAction(host, id, action, first, second, extendSelection, text)

    /** As `AndroidComposeView`'s input mode manager, asked for keyboard mode. Rust's too. */
    fun requestKeyboardMode(): Boolean = if (isInTouchMode) requestFocusFromTouch() else true

    override fun onDraw(canvas: Canvas) {
        if (host == 0L) return
        val taken = ComposeRsNative.takeDrawing(host)
        if (taken and ComposeRsNative.NEW_DRAWING_BUFFER != 0L || drawing == null) {
            drawing = ComposeRsNative.drawingBuffer(host)
        }
        val drawing = drawing ?: return
        drawing.limit((taken and 0xFFFFFFFFL).toInt())
        Trace.beginSection("compose-rs: replay")
        replayer.replay(canvas, drawing)
        Trace.endSection()
    }

    // ---- Touch ---------------------------------------------------------------------------

    // What the pointers of a MotionEvent cross JNI in, reused: each one's id and tool, then x, y
    // and pressure.
    private var touchInts = IntArray(MAX_TOUCHES * 2)
    private var touchFloats = FloatArray(MAX_TOUCHES * 3)

    /** `event`'s pointers together: at its historical `sample`, or as it is. */
    private fun sendTouches(event: MotionEvent, action: Int, sample: Int, age: Long) {
        val count = event.pointerCount
        if (count * 2 > touchInts.size) {
            touchInts = IntArray(count * 2)
            touchFloats = FloatArray(count * 3)
        }
        for (index in 0 until count) {
            touchInts[index * 2] = event.getPointerId(index)
            touchInts[index * 2 + 1] = event.getToolType(index)
            if (sample < 0) {
                touchFloats[index * 3] = event.getX(index)
                touchFloats[index * 3 + 1] = event.getY(index)
                touchFloats[index * 3 + 2] = event.getPressure(index)
            } else {
                touchFloats[index * 3] = event.getHistoricalX(index, sample)
                touchFloats[index * 3 + 1] = event.getHistoricalY(index, sample)
                touchFloats[index * 3 + 2] = event.getHistoricalPressure(index, sample)
            }
        }
        ComposeRsNative.touches(
            host,
            action,
            event.actionIndex,
            count,
            touchInts,
            touchFloats,
            event.buttonState,
            event.metaState,
            age,
        )
    }

    // ---- A mouse's hover exit, as `AndroidComposeView`: a press comes with one -----------------

    /** A hover exit waits this long for what follows, as Compose's one frame at 120 Hz. */
    private var hoverExitPending = false
    private val sendHoverExit = Runnable {
        hoverExitPending = false
        if (host != 0L) ComposeRsNative.hoverExit(host)
    }

    /** What came next is what the hover exit was for if [dropped]; else it goes first. */
    private fun settleHoverExit(dropped: Boolean) {
        if (!hoverExitPending) return
        removeCallbacks(sendHoverExit)
        if (dropped) hoverExitPending = false else sendHoverExit.run()
    }

    override fun dispatchTouchEvent(event: MotionEvent): Boolean {
        settleHoverExit(event.actionMasked == MotionEvent.ACTION_DOWN)
        return super.dispatchTouchEvent(event)
    }

    override fun dispatchGenericMotionEvent(event: MotionEvent): Boolean {
        settleHoverExit(event.actionMasked == MotionEvent.ACTION_SCROLL)
        return super.dispatchGenericMotionEvent(event)
    }

    override fun onTouchEvent(event: MotionEvent): Boolean {
        if (host == 0L) return false
        val now = SystemClock.uptimeMillis()
        fun age(time: Long) = (now - time).coerceAtLeast(0L) * 1_000_000L
        when (event.actionMasked) {
            MotionEvent.ACTION_DOWN, MotionEvent.ACTION_POINTER_DOWN ->
                sendTouches(event, 0, -1, age(event.eventTime))
            MotionEvent.ACTION_MOVE -> {
                // The samples batched since the last event, then the latest, at their times: a
                // fling's velocity comes from all of them.
                for (sample in 0 until event.historySize) {
                    sendTouches(event, 1, sample, age(event.getHistoricalEventTime(sample)))
                }
                sendTouches(event, 1, -1, age(event.eventTime))
            }
            MotionEvent.ACTION_UP, MotionEvent.ACTION_POINTER_UP ->
                sendTouches(event, 2, -1, age(event.eventTime))
            MotionEvent.ACTION_CANCEL -> {
                for (index in 0 until event.pointerCount) {
                    ComposeRsNative.touchCancel(host, event.getPointerId(index), event.getToolType(index))
                }
            }
        }
        // A timeout a gesture started (a long press): a tick when it ends.
        ComposeRsRuntime.schedule(ComposeRsNative.nextTimer())
        return true
    }

    /** A mouse moving with no button down: what's under it hears `Enter` and `Exit`. */
    override fun onHoverEvent(event: MotionEvent): Boolean {
        if (host != 0L && event.getToolType(0) == MotionEvent.TOOL_TYPE_MOUSE) {
            when (event.actionMasked) {
                MotionEvent.ACTION_HOVER_ENTER, MotionEvent.ACTION_HOVER_MOVE -> {
                    sendTouches(event, 4, -1, 0L)
                    return true
                }
                MotionEvent.ACTION_HOVER_EXIT -> {
                    // A button's press caused it: nothing's left.
                    if (event.buttonState != 0) return true
                    // A press may follow, or a wheel: only what follows tells.
                    hoverExitPending = true
                    postDelayed(sendHoverExit, HOVER_EXIT_DELAY_MILLIS)
                    return true
                }
            }
        }
        return super.onHoverEvent(event)
    }

    /**
     * A mouse wheel (or a trackpad's scroll), in pixels, by the view configuration's scroll factors,
     * as Compose's `AndroidConfig` (D1450).
     */
    override fun onGenericMotionEvent(event: MotionEvent): Boolean {
        // Another of a mouse's buttons, its pointer being down already.
        if (host != 0L &&
            (event.actionMasked == MotionEvent.ACTION_BUTTON_PRESS ||
                event.actionMasked == MotionEvent.ACTION_BUTTON_RELEASE) &&
            event.getToolType(0) == MotionEvent.TOOL_TYPE_MOUSE
        ) {
            sendTouches(event, 3, -1, 0L)
            return true
        }
        if (host != 0L && event.actionMasked == MotionEvent.ACTION_SCROLL) {
            val configuration = ViewConfiguration.get(context)
            ComposeRsNative.scroll(
                host,
                event.x,
                event.y,
                // Compose's (-H h, +V v) is in drag-like coordinates; these are positive forward.
                event.getAxisValue(MotionEvent.AXIS_HSCROLL) *
                    configuration.scaledHorizontalScrollFactor,
                -event.getAxisValue(MotionEvent.AXIS_VSCROLL) *
                    configuration.scaledVerticalScrollFactor,
                event.metaState,
            )
            return true
        }
        return super.onGenericMotionEvent(event)
    }

    // ---- Keys and text -------------------------------------------------------------------

    private fun keyCode(code: Int): Int = when (code) {
        KeyEvent.KEYCODE_ENTER, KeyEvent.KEYCODE_NUMPAD_ENTER -> 1
        KeyEvent.KEYCODE_SPACE -> 2
        KeyEvent.KEYCODE_TAB -> 3
        KeyEvent.KEYCODE_DEL -> 4
        KeyEvent.KEYCODE_FORWARD_DEL -> 5
        KeyEvent.KEYCODE_ESCAPE -> 6
        KeyEvent.KEYCODE_DPAD_LEFT -> 7
        KeyEvent.KEYCODE_DPAD_RIGHT -> 8
        KeyEvent.KEYCODE_DPAD_UP -> 9
        KeyEvent.KEYCODE_DPAD_DOWN -> 10
        KeyEvent.KEYCODE_MOVE_HOME -> 11
        KeyEvent.KEYCODE_MOVE_END -> 12
        KeyEvent.KEYCODE_PAGE_UP -> 13
        KeyEvent.KEYCODE_PAGE_DOWN -> 14
        KeyEvent.KEYCODE_DPAD_CENTER -> 15
        KeyEvent.KEYCODE_BACK -> 16
        KeyEvent.KEYCODE_NAVIGATE_NEXT -> 17
        KeyEvent.KEYCODE_NAVIGATE_PREVIOUS -> 18
        else -> 0
    }

    internal fun sendKey(event: KeyEvent): Boolean {
        if (host == 0L) return false
        val down = event.action == KeyEvent.ACTION_DOWN
        if (event.action == KeyEvent.ACTION_MULTIPLE) return false
        // The key's character without Ctrl, Alt or Meta (which map to none), so a shortcut's
        // key (Ctrl+C) is known; Rust types no text while they're held.
        val shortcutMask = KeyEvent.META_CTRL_MASK or KeyEvent.META_ALT_MASK or KeyEvent.META_META_MASK
        val consumed = ComposeRsNative.key(
            host,
            keyCode(event.keyCode),
            event.getUnicodeChar(event.metaState and shortcutMask.inv()),
            down,
            event.metaState,
        )
        ComposeRsRuntime.schedule(ComposeRsNative.nextTimer())
        return consumed
    }

    override fun onKeyDown(keyCode: Int, event: KeyEvent): Boolean {
        // The back key, where no dispatcher takes back (no AndroidX, no predictive back): the
        // content's, on its release, while a handler is enabled.
        if (keyCode == KeyEvent.KEYCODE_BACK) {
            // Back is `Exit` first, if the focus has somewhere to go, as Compose's key handling.
            if (sendKey(event)) return true
            if (back?.handlesKeys == true) {
                event.startTracking()
                return true
            }
            return super.onKeyDown(keyCode, event)
        }
        return sendKey(event) || super.onKeyDown(keyCode, event)
    }

    override fun onKeyUp(keyCode: Int, event: KeyEvent): Boolean {
        if (keyCode == KeyEvent.KEYCODE_BACK) {
            if (back?.handlesKeys == true && event.isTracking && !event.isCanceled) {
                handleBack()
                return true
            }
            return super.onKeyUp(keyCode, event)
        }
        return sendKey(event) || super.onKeyUp(keyCode, event)
    }

    override fun onRtlPropertiesChanged(layoutDirection: Int) {
        super.onRtlPropertiesChanged(layoutDirection)
        if (host != 0L) {
            ComposeRsNative.setRtl(host, layoutDirection == LAYOUT_DIRECTION_RTL)
            requestFrame()
        }
    }

    private fun updateTextInput(active: Boolean, reset: Boolean) {
        if (active != textInput) {
            textInput = active
            if (active) {
                requestFocus()
                inputMethods.restartInput(this)
                inputMethods.showSoftInput(this, 0)
            } else {
                inputMethods.hideSoftInputFromWindow(windowToken, 0)
                inputMethods.restartInput(this)
            }
        } else if (reset && active) {
            inputMethods.restartInput(this)
        }
    }

    override fun onCheckIsTextEditor(): Boolean = textInput

    /**
     * The focused field's keyboard: its type, action key, capitalization, auto-correction and
     * lines, as Compose's `EditorInfo.update` (computed in Rust, `input_method.rs`).
     */
    override fun onCreateInputConnection(info: EditorInfo): InputConnection? {
        if (!textInput || host == 0L) return null
        val options = ComposeRsNative.textInputOptions(host)
        info.inputType = options.toInt()
        info.imeOptions = (options shr 32).toInt()
        // What the keyboard starts from, as Compose's `EditorInfo.update`.
        val state = IntArray(4)
        if (ComposeRsNative.imeInputState(host, state)) {
            info.initialSelStart = state[0]
            info.initialSelEnd = state[1]
            if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.R) {
                ComposeRsNative.imeText(host)?.let { info.setInitialSurroundingText(it) }
            }
        }
        closeCollectedConnections()
        val connection = ComposeRsInputConnection(this, ComposeRsNative.imeOpen(host))
        openConnections += ConnectionReference(connection, collectedConnections)
        ComposeRsInputConnection.log { "onCreateInputConnection(sel=${info.initialSelStart},${info.initialSelEnd}, id=${connection.id})" }
        return connection
    }

    /**
     * Connections Android threw away never close: those it has let go are closed here, so the cap
     * on the open ones in Rust isn't reached by those.
     */
    private fun closeCollectedConnections() {
        while (true) {
            val collected = collectedConnections.poll() as? ConnectionReference ?: return
            openConnections -= collected
            ComposeRsNative.imeClose(host, collected.id)
        }
    }

    private class ConnectionReference(
        connection: ComposeRsInputConnection,
        queue: ReferenceQueue<ComposeRsInputConnection>,
    ) : WeakReference<ComposeRsInputConnection>(connection, queue) {
        val id = connection.id
    }

    /**
     * Tells the keyboard what it hasn't heard of the text and the cursor, as Compose's
     * `updateInputState` and `LegacyCursorAnchorInfoController` (after its edits, and after frames).
     */
    internal fun updateInputMethod() {
        if (host == 0L) return
        val update = IntArray(6)
        if (ComposeRsNative.imeTakeSelectionUpdate(host, update)) {
            ComposeRsInputConnection.log { "updateSelection(${update[0]}, ${update[1]}, ${update[2]}, ${update[3]})" }
            inputMethods.updateSelection(this, update[0], update[1], update[2], update[3])
            if (update[4] != 0) {
                extractedText()?.let { inputMethods.updateExtractedText(this, update[5], it) }
            }
        }
        val anchor = ComposeRsNative.imeTakeCursorAnchor(host) ?: return
        if (inputMethods.isActive(this)) {
            ComposeRsInputConnection.log { "updateCursorAnchorInfo(${anchor.size} floats)" }
            inputMethods.updateCursorAnchorInfo(this, cursorAnchorInfo(anchor))
        }
    }

    /** The field's text for the keyboard, as Compose's `toExtractedText`. */
    internal fun extractedText(): ExtractedText? {
        if (host == 0L) return null
        val text = ComposeRsNative.imeText(host) ?: return null
        val state = IntArray(4)
        ComposeRsNative.imeInputState(host, state)
        return ExtractedText().also {
            it.text = text
            it.startOffset = 0
            it.partialEndOffset = text.length
            it.partialStartOffset = -1 // the whole text
            it.selectionStart = state[0]
            it.selectionEnd = state[1]
            it.flags = if ('\n' in text) 0 else ExtractedText.FLAG_SINGLE_LINE
        }
    }

    /** The field's text and cursor as Android's, from `input_method::encode_cursor_anchor`. */
    private fun cursorAnchorInfo(data: FloatArray): CursorAnchorInfo {
        val location = IntArray(2)
        getLocationOnScreen(location)
        val builder = CursorAnchorInfo.Builder()
        builder.setMatrix(Matrix().apply { setTranslate(location[0] + data[0], location[1] + data[1]) })
        builder.setSelectionRange(data[2].toInt(), data[3].toInt())
        val (compositionStart, compositionEnd) = data[4].toInt() to data[5].toInt()
        if (compositionStart >= 0 && compositionStart < compositionEnd) {
            val text = ComposeRsNative.imeText(host)
            if (text != null && compositionEnd <= text.length) {
                builder.setComposingText(compositionStart, text.substring(compositionStart, compositionEnd))
            }
        }
        if (data[6] != 0f) builder.setInsertionMarkerLocation(data[7], data[8], data[9], data[9], data[10].toInt())
        if (data[11] != 0f && Build.VERSION.SDK_INT >= Build.VERSION_CODES.TIRAMISU) {
            val bounds = RectF(data[12], data[13], data[14], data[15])
            builder.setEditorBoundsInfo(
                EditorBoundsInfo.Builder().setEditorBounds(bounds).setHandwritingBounds(bounds).build()
            )
        }
        var at = CURSOR_ANCHOR_HEADER
        repeat(data[16].toInt()) {
            builder.addCharacterBounds(
                data[at].toInt(), data[at + 1], data[at + 2], data[at + 3], data[at + 4], data[at + 5].toInt()
            )
            at += 6
        }
        repeat(data[17].toInt()) {
            if (data[18] != 0f && Build.VERSION.SDK_INT >= Build.VERSION_CODES.UPSIDE_DOWN_CAKE) {
                builder.addVisibleLineBounds(data[at], data[at + 1], data[at + 2], data[at + 3])
            }
            at += 4
        }
        return builder.build()
    }

    /** The keyboard's Cut, Copy or Paste, as Compose's synthesized keys. */
    internal fun sendShortcut(keyCode: Int) {
        sendKey(KeyEvent(0, 0, KeyEvent.ACTION_DOWN, keyCode, 0, KeyEvent.META_CTRL_ON))
        sendKey(KeyEvent(0, 0, KeyEvent.ACTION_UP, keyCode, 0, KeyEvent.META_CTRL_ON))
    }

    // ---- The clipboard (Rust calls these) ------------------------------------------------

    fun clipboardText(): String? {
        val clipboard = context.getSystemService(ClipboardManager::class.java)
        return clipboard.primaryClip?.takeIf { it.itemCount > 0 }?.getItemAt(0)?.coerceToText(context)?.toString()
    }

    fun setClipboardText(text: String) {
        val clipboard = context.getSystemService(ClipboardManager::class.java)
        clipboard.setPrimaryClip(ClipData.newPlainText("text", text))
    }

    // ---- Links and haptics (Rust calls these: `LOCAL_URI_HANDLER`, `LOCAL_HAPTIC_FEEDBACK`) --

    /**
     * Opens [uri] with an `ACTION_VIEW` intent, as Compose's `AndroidUriHandler`; Rust checked
     * its scheme (`PlatformUriHandler`). Rust calls it in the middle of a call into Rust: posted
     * to the main looper (D226). A failure (nothing opens it, a `file:` URI the app may not
     * expose, a permission) is logged, never thrown at the looper.
     */
    fun openUri(uri: String) {
        mainHandler.post {
            val intent = Intent(Intent.ACTION_VIEW, Uri.parse(uri))
            if (findActivity(context) == null) intent.addFlags(Intent.FLAG_ACTIVITY_NEW_TASK)
            try {
                context.startActivity(intent)
            } catch (exception: RuntimeException) {
                val scheme = Uri.parse(uri).scheme
                Log.w("compose-rs", "not opening a $scheme: URI: ${exception.javaClass.simpleName}")
            }
        }
    }

    /** The activity [context] wraps, if any (an `AndroidView`'s context is often a wrapper). */
    private fun findActivity(context: Context): android.app.Activity? {
        var current: Context? = context
        while (current is android.content.ContextWrapper) {
            if (current is android.app.Activity) return current
            current = current.baseContext
        }
        return null
    }

    /**
     * Performs the haptic feedback of [type] (`HapticFeedbackType`'s index in Compose's order),
     * with Compose's fallbacks before the API level that has the constant. Posted, as [openUri].
     */
    fun performHapticFeedbackType(type: Int) {
        val constant = hapticFeedbackConstant(type) ?: return
        mainHandler.post { performHapticFeedback(constant) }
    }

    private fun hapticFeedbackConstant(type: Int): Int? {
        val sdk = Build.VERSION.SDK_INT
        return when (type) {
            0 -> HapticFeedbackConstants.LONG_PRESS
            1 -> HapticFeedbackConstants.TEXT_HANDLE_MOVE
            2 -> if (sdk >= 30) HapticFeedbackConstants.CONFIRM else HapticFeedbackConstants.VIRTUAL_KEY
            3 -> HapticFeedbackConstants.CONTEXT_CLICK
            4 -> if (sdk >= 30) HapticFeedbackConstants.GESTURE_END else HapticFeedbackConstants.CONTEXT_CLICK
            5 -> if (sdk >= 34) HapticFeedbackConstants.GESTURE_THRESHOLD_ACTIVATE else HapticFeedbackConstants.CONTEXT_CLICK
            6 -> HapticFeedbackConstants.KEYBOARD_TAP
            7 -> if (sdk >= 30) HapticFeedbackConstants.REJECT else HapticFeedbackConstants.LONG_PRESS
            8 -> if (sdk >= 34) HapticFeedbackConstants.SEGMENT_FREQUENT_TICK else HapticFeedbackConstants.CLOCK_TICK
            9 -> if (sdk >= 34) HapticFeedbackConstants.SEGMENT_TICK else HapticFeedbackConstants.CLOCK_TICK
            10 -> if (sdk >= 34) HapticFeedbackConstants.TOGGLE_OFF else HapticFeedbackConstants.VIRTUAL_KEY_RELEASE
            11 -> if (sdk >= 34) HapticFeedbackConstants.TOGGLE_ON else HapticFeedbackConstants.VIRTUAL_KEY
            12 -> HapticFeedbackConstants.VIRTUAL_KEY
            else -> null
        }
    }

    private companion object {
        /** The pointers `sendTouches`'s arrays start with room for. */
        const val MAX_TOUCHES = 10

        /** As `ONE_FRAME_120_HERTZ_IN_MILLISECONDS`. */
        const val HOVER_EXIT_DELAY_MILLIS = 8L
        const val SUPER_STATE = "rs.compose.android.super"

        /** `ComposeRsNative.performAction`'s for a request for the focus. */
        const val ACTION_REQUEST_FOCUS = 1
        const val COMPOSE_RS_STATE = "rs.compose.android.state"

        /** What recording paragraphs while idle leaves before the next frame, as Rust's prefetch. */
        const val RECORD_MARGIN_NANOS = 500_000L

        /** Idle times in a row without time before [prefetch] waits for a frame (D340). */
        const val MOST_IDLE_TIMES_WITHOUT_TIME = 3

        /** `input_method::CURSOR_ANCHOR_HEADER`. */
        const val CURSOR_ANCHOR_HEADER = 19
    }
}

/**
 * The soft keyboard's connection, as Compose's `RecordingInputConnection`: everything goes to Rust
 * as it came (`input_method.rs`, host-tested, D1664), in UTF-16 units. Once closed it takes
 * nothing more. `adb shell setprop log.tag.ComposeRsIme DEBUG` logs every call, with the text.
 */
internal class ComposeRsInputConnection(
    private val view: ComposeRsView,
    val id: Long,
) : InputConnection {
    /** The view's host as it is now: 0 once it's gone, which Rust answers false to. */
    private val host: Long get() = view.nativeHost

    private inline fun edit(name: String, arguments: () -> String, block: () -> Boolean): Boolean {
        val result = block()
        log { "#$id $name(${arguments()}) -> $result" }
        view.updateInputMethod()
        return result
    }

    private inline fun <T> read(name: String, arguments: () -> String, block: () -> T): T {
        val result = block()
        log { "#$id $name(${arguments()}) -> $result" }
        return result
    }

    private inline fun ifActive(block: () -> Boolean): Boolean =
        ComposeRsNative.imeIsActive(host, id) && block()

    override fun beginBatchEdit(): Boolean =
        edit("beginBatchEdit", { "" }) { ComposeRsNative.imeBeginBatchEdit(host, id) }

    override fun endBatchEdit(): Boolean =
        edit("endBatchEdit", { "" }) { ComposeRsNative.imeEndBatchEdit(host, id) }

    override fun commitText(text: CharSequence?, newCursorPosition: Int): Boolean =
        edit("commitText", { "\"$text\", $newCursorPosition" }) {
            ComposeRsNative.imeCommitText(host, id, text.toString(), newCursorPosition)
        }

    override fun setComposingText(text: CharSequence?, newCursorPosition: Int): Boolean =
        edit("setComposingText", { "\"$text\", $newCursorPosition" }) {
            ComposeRsNative.imeSetComposingText(host, id, text.toString(), newCursorPosition)
        }

    override fun setComposingRegion(start: Int, end: Int): Boolean =
        edit("setComposingRegion", { "$start, $end" }) {
            ComposeRsNative.imeSetComposingRegion(host, id, start, end)
        }

    override fun finishComposingText(): Boolean =
        edit("finishComposingText", { "" }) { ComposeRsNative.imeFinishComposingText(host, id) }

    override fun deleteSurroundingText(beforeLength: Int, afterLength: Int): Boolean =
        edit("deleteSurroundingText", { "$beforeLength, $afterLength" }) {
            ComposeRsNative.imeDeleteSurroundingText(host, id, beforeLength, afterLength)
        }

    override fun deleteSurroundingTextInCodePoints(beforeLength: Int, afterLength: Int): Boolean =
        edit("deleteSurroundingTextInCodePoints", { "$beforeLength, $afterLength" }) {
            ComposeRsNative.imeDeleteSurroundingTextInCodePoints(host, id, beforeLength, afterLength)
        }

    override fun setSelection(start: Int, end: Int): Boolean =
        edit("setSelection", { "$start, $end" }) { ComposeRsNative.imeSetSelection(host, id, start, end) }

    override fun getTextBeforeCursor(maxChars: Int, flags: Int): CharSequence =
        read("getTextBeforeCursor", { "$maxChars, $flags" }) {
            ComposeRsNative.imeTextBeforeCursor(host, maxChars).orEmpty()
        }

    override fun getTextAfterCursor(maxChars: Int, flags: Int): CharSequence =
        read("getTextAfterCursor", { "$maxChars, $flags" }) {
            ComposeRsNative.imeTextAfterCursor(host, maxChars).orEmpty()
        }

    override fun getSelectedText(flags: Int): CharSequence? =
        read("getSelectedText", { "$flags" }) { ComposeRsNative.imeSelectedText(host) }

    override fun getCursorCapsMode(reqModes: Int): Int =
        read("getCursorCapsMode", { "$reqModes" }) {
            val text = ComposeRsNative.imeText(host).orEmpty()
            val state = IntArray(4)
            ComposeRsNative.imeInputState(host, state)
            TextUtils.getCapsMode(text, state[0], reqModes)
        }

    override fun getExtractedText(request: ExtractedTextRequest?, flags: Int): ExtractedText? =
        read("getExtractedText", { "${request?.token}, $flags" }) {
            val monitor = flags and InputConnection.GET_EXTRACTED_TEXT_MONITOR != 0
            ComposeRsNative.imeSetExtractedTextMonitor(host, id, monitor, request?.token ?: 0)
            view.extractedText()
        }

    override fun requestCursorUpdates(cursorUpdateMode: Int): Boolean =
        edit("requestCursorUpdates", { "$cursorUpdateMode" }) {
            ComposeRsNative.imeRequestCursorUpdates(host, id, cursorUpdateMode, Build.VERSION.SDK_INT)
        }

    /** Select all, or the keyboard's Cut, Copy and Paste as keys, as Compose's; never "handled". */
    override fun performContextMenuAction(id: Int): Boolean =
        edit("performContextMenuAction", { "$id" }) {
            when (id) {
                android.R.id.selectAll -> ComposeRsNative.imeSelectAll(host, this.id)
                android.R.id.cut -> ifActive { view.sendShortcut(KeyEvent.KEYCODE_X); true }
                android.R.id.copy -> ifActive { view.sendShortcut(KeyEvent.KEYCODE_C); true }
                android.R.id.paste -> ifActive { view.sendShortcut(KeyEvent.KEYCODE_V); true }
                else -> {}
            }
            false
        }

    override fun commitCompletion(text: CompletionInfo?): Boolean =
        read("commitCompletion", { "${text?.text}" }) { false }

    override fun commitCorrection(correctionInfo: CorrectionInfo?): Boolean =
        read("commitCorrection", { "$correctionInfo" }) { ComposeRsNative.imeCommitCorrection(host, id) }

    override fun clearMetaKeyStates(states: Int): Boolean =
        read("clearMetaKeyStates", { "$states" }) { false }

    override fun performPrivateCommand(action: String?, data: Bundle?): Boolean =
        read("performPrivateCommand", { "$action" }) { ifActive { true } }

    override fun commitContent(inputContentInfo: InputContentInfo, flags: Int, opts: Bundle?): Boolean =
        read("commitContent", { "$flags" }) { false }

    /**
     * The connection closes (the keyboard restarts input, focus moves): what it was composing stays
     * the field's, as Compose's.
     */
    override fun closeConnection() {
        log { "#$id closeConnection()" }
        ComposeRsNative.imeClose(host, id)
    }

    /** The default handler, as Compose's. */
    override fun getHandler(): Handler? = read("getHandler", { "" }) { null }

    /** Ignored by Android, as the API says. */
    override fun reportFullscreenMode(enabled: Boolean): Boolean =
        read("reportFullscreenMode", { "$enabled" }) { false }

    override fun sendKeyEvent(event: KeyEvent): Boolean =
        edit("sendKeyEvent", { "$event" }) { ifActive { view.sendKey(event); true } }

    /**
     * The keyboard's action key (Done, Search, Next…), as Compose's `RecordingInputConnection`:
     * always taken while open. (Enter, which keyboards send as a key, performs a single line's
     * action in the field itself.)
     */
    override fun performEditorAction(editorAction: Int): Boolean =
        edit("performEditorAction", { "$editorAction" }) {
            ifActive {
                ComposeRsNative.imePerformEditorAction(host, id, editorAction)
                ComposeRsRuntime.schedule(ComposeRsNative.nextTimer())
                true
            }
        }

    companion object {
        /** Temporary: every call and its arguments, to read a keyboard's behaviour from logcat. */
        const val LOG_TAG = "ComposeRsIme"

        inline fun log(message: () -> String) {
            if (Log.isLoggable(LOG_TAG, Log.DEBUG)) {
                val text = message()
                Log.d(LOG_TAG, text)
            }
        }
    }
}
