package rs.compose.android

import android.content.ClipData
import android.content.ClipboardManager
import android.content.Context
import android.content.res.Configuration
import android.graphics.Canvas
import android.os.SystemClock
import android.os.Trace
import android.text.InputType
import android.view.Choreographer
import android.view.KeyEvent
import android.view.MotionEvent
import android.view.View
import android.view.accessibility.AccessibilityManager
import android.view.accessibility.AccessibilityNodeProvider
import android.view.inputmethod.BaseInputConnection
import android.view.inputmethod.EditorInfo
import android.view.inputmethod.InputConnection
import android.view.inputmethod.InputMethodManager

/**
 * A view showing a compose-rs app (`compose_android::export_apps!` names them), as a plain
 * `View`: in a Kotlin Compose UI through `AndroidView`, it's transformed, clipped and scrolled as
 * any content (docs/07-platforms.md, "Embedding").
 *
 * On a Choreographer frame (asked for when the app's state changes), Rust recomposes, lays out
 * and records a display list; [onDraw] replays it on the view's (hardware) canvas. The
 * composition lives while the view is attached to a window.
 */
class ComposeRsView(context: Context, private val app: String) : View(context) {
    private val measurer = TextMeasurer()
    private val replayer = Replayer(measurer)
    private var host = 0L
    private var frameRequested = false
    private var textInput = false
    private val inputMethods = context.getSystemService(InputMethodManager::class.java)

    private val frameCallback = Choreographer.FrameCallback { frame() }
    private val accessibility = Accessibility(this)
    private val accessibilityManager = context.getSystemService(AccessibilityManager::class.java)
    /** A change of semantics is told to accessibility services at most this often. */
    private var semanticsChangePosted = false
    private val semanticsChanged = Runnable {
        semanticsChangePosted = false
        accessibility.invalidate()
    }

    init {
        isFocusable = true
        isFocusableInTouchMode = true
    }

    override fun onAttachedToWindow() {
        super.onAttachedToWindow()
        ComposeRsRuntime.attach(this)
        if (width > 0 && height > 0) create()
    }

    override fun onDetachedFromWindow() {
        ComposeRsRuntime.detach(this)
        destroy()
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
        )
        requestFrame()
    }

    private fun destroy() {
        if (host == 0L) return
        Choreographer.getInstance().removeFrameCallback(frameCallback)
        frameRequested = false
        ComposeRsNative.destroy(host)
        host = 0L
        measurer.clear()
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
            requestFrame()
        }
    }

    /** Asks for a frame (Rust calls it when the app's state changes). */
    fun requestFrame() {
        if (frameRequested || host == 0L) return
        frameRequested = true
        Choreographer.getInstance().postFrameCallback(frameCallback)
    }

    private fun frame() {
        frameRequested = false
        if (host == 0L) return
        val result = ComposeRsNative.frame(host)
        if (result and Frame.DREW != 0L) invalidate()
        if (result and Frame.AGAIN != 0L) requestFrame()
        ComposeRsRuntime.schedule(Frame.nextTimer(result))
        updateTextInput(result and Frame.TEXT_INPUT != 0L, result and Frame.RESET_INPUT != 0L)
        if (result and Frame.SEMANTICS != 0L && accessibilityManager.isEnabled && !semanticsChangePosted) {
            semanticsChangePosted = true
            postDelayed(semanticsChanged, 100)
        }
    }

    // ---- Accessibility -------------------------------------------------------------------

    override fun getAccessibilityNodeProvider(): AccessibilityNodeProvider = accessibility

    override fun dispatchHoverEvent(event: MotionEvent): Boolean {
        if (accessibilityManager.isTouchExplorationEnabled && accessibility.hover(event)) return true
        return super.dispatchHoverEvent(event)
    }

    internal fun semantics() = if (host != 0L) ComposeRsNative.semantics(host) else null

    internal fun performSemanticsAction(id: Int, action: Int, value: Float): Boolean =
        host != 0L && ComposeRsNative.performAction(host, id, action, value)

    override fun onDraw(canvas: Canvas) {
        if (host == 0L) return
        val drawing = ComposeRsNative.takeDrawing(host) ?: return
        Trace.beginSection("compose-rs: replay")
        replayer.replay(canvas, drawing)
        Trace.endSection()
    }

    // ---- Touch ---------------------------------------------------------------------------

    override fun onTouchEvent(event: MotionEvent): Boolean {
        if (host == 0L) return false
        val now = SystemClock.uptimeMillis()
        fun age(time: Long) = (now - time).coerceAtLeast(0L) * 1_000_000L
        when (event.actionMasked) {
            MotionEvent.ACTION_DOWN, MotionEvent.ACTION_POINTER_DOWN -> {
                val index = event.actionIndex
                ComposeRsNative.touch(host, 0, event.getPointerId(index), event.getX(index), event.getY(index), age(event.eventTime))
            }
            MotionEvent.ACTION_MOVE -> {
                // The samples batched since the last event, then the latest, at their times: a
                // fling's velocity comes from all of them.
                for (sample in 0 until event.historySize) {
                    val time = age(event.getHistoricalEventTime(sample))
                    for (index in 0 until event.pointerCount) {
                        ComposeRsNative.touch(
                            host,
                            1,
                            event.getPointerId(index),
                            event.getHistoricalX(index, sample),
                            event.getHistoricalY(index, sample),
                            time,
                        )
                    }
                }
                for (index in 0 until event.pointerCount) {
                    ComposeRsNative.touch(host, 1, event.getPointerId(index), event.getX(index), event.getY(index), age(event.eventTime))
                }
            }
            MotionEvent.ACTION_UP, MotionEvent.ACTION_POINTER_UP -> {
                val index = event.actionIndex
                ComposeRsNative.touch(host, 2, event.getPointerId(index), event.getX(index), event.getY(index), age(event.eventTime))
            }
            MotionEvent.ACTION_CANCEL -> {
                for (index in 0 until event.pointerCount) {
                    ComposeRsNative.touch(host, 3, event.getPointerId(index), 0f, 0f, 0L)
                }
            }
        }
        // A timeout a gesture started (a long press): a tick when it ends.
        ComposeRsRuntime.schedule(ComposeRsNative.nextTimer())
        return true
    }

    /** A mouse wheel (or a trackpad's scroll), in pixels, as Compose's scroll events. */
    override fun onGenericMotionEvent(event: MotionEvent): Boolean {
        if (host != 0L && event.actionMasked == MotionEvent.ACTION_SCROLL) {
            val step = 64f * resources.displayMetrics.density
            ComposeRsNative.scroll(
                host,
                event.x,
                event.y,
                -event.getAxisValue(MotionEvent.AXIS_HSCROLL) * step,
                -event.getAxisValue(MotionEvent.AXIS_VSCROLL) * step,
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

    override fun onKeyDown(keyCode: Int, event: KeyEvent): Boolean = sendKey(event) || super.onKeyDown(keyCode, event)

    override fun onKeyUp(keyCode: Int, event: KeyEvent): Boolean = sendKey(event) || super.onKeyUp(keyCode, event)

    internal fun commitText(text: CharSequence) {
        if (host != 0L) ComposeRsNative.text(host, text.toString())
    }

    internal fun composeText(text: CharSequence, cursor: Int) {
        if (host != 0L) ComposeRsNative.preedit(host, text.toString(), cursor)
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

    override fun onCreateInputConnection(info: EditorInfo): InputConnection? {
        if (!textInput || host == 0L) return null
        val kind = ComposeRsNative.textInputKind(host)
        info.inputType = if (kind and 1 != 0) {
            // A password: no suggestions learned from it.
            InputType.TYPE_CLASS_TEXT or InputType.TYPE_TEXT_VARIATION_PASSWORD
        } else {
            InputType.TYPE_CLASS_TEXT
        }
        info.imeOptions = EditorInfo.IME_FLAG_NO_FULLSCREEN or EditorInfo.IME_FLAG_NO_EXTRACT_UI
        return ComposeRsInputConnection(this)
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
}

/**
 * The soft keyboard's edits, as compose-rs takes them: committed text, the composing text (shown
 * in place, underlined), deletions as Backspace and Delete keys. The field's text stays on the
 * Rust side, so the keyboard sees none (as a terminal's input connection).
 */
internal class ComposeRsInputConnection(private val view: ComposeRsView) : BaseInputConnection(view, false) {
    private var composing: CharSequence = ""

    override fun commitText(text: CharSequence, newCursorPosition: Int): Boolean {
        if (composing.isNotEmpty()) {
            composing = ""
            view.composeText("", -1)
        }
        view.commitText(text)
        return true
    }

    override fun setComposingText(text: CharSequence, newCursorPosition: Int): Boolean {
        composing = text
        view.composeText(text, -1)
        return true
    }

    override fun finishComposingText(): Boolean {
        if (composing.isNotEmpty()) {
            val text = composing
            composing = ""
            view.composeText("", -1)
            view.commitText(text)
        }
        return true
    }

    override fun deleteSurroundingText(beforeLength: Int, afterLength: Int): Boolean {
        repeat(beforeLength) { key(KeyEvent.KEYCODE_DEL) }
        repeat(afterLength) { key(KeyEvent.KEYCODE_FORWARD_DEL) }
        return true
    }

    /**
     * The connection closes (the keyboard restarts input, focus moves): what it was composing
     * is the field's to commit or drop (it already did), not to commit again.
     */
    override fun closeConnection() {
        composing = ""
        super.closeConnection()
    }

    override fun sendKeyEvent(event: KeyEvent): Boolean {
        view.sendKey(event)
        return true
    }

    private fun key(code: Int) {
        view.sendKey(KeyEvent(KeyEvent.ACTION_DOWN, code))
        view.sendKey(KeyEvent(KeyEvent.ACTION_UP, code))
    }
}
