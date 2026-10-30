package rs.compose.android

import android.graphics.Rect
import android.os.Build
import android.os.Bundle
import android.util.SparseArray
import android.view.MotionEvent
import android.view.View
import android.view.accessibility.AccessibilityEvent
import android.view.accessibility.AccessibilityManager
import android.view.accessibility.AccessibilityNodeInfo
import android.view.accessibility.AccessibilityNodeInfo.AccessibilityAction
import android.view.accessibility.AccessibilityNodeProvider
import java.nio.ByteBuffer
import java.nio.ByteOrder

/**
 * compose-rs's semantics as virtual views (`crates/compose-android/src/accessibility.rs`): what
 * TalkBack, uiautomator and other services read and act on. The tree is read from Rust when a
 * service asks after a change, whether a service was on at the change or connected since: a
 * change only marks the nodes read stale, as Compose's `currentSemanticsNodesInvalidated`, and
 * the services on hear it (D630).
 */
internal class Accessibility(private val view: ComposeRsView) :
    AccessibilityNodeProvider(),
    AccessibilityManager.AccessibilityStateChangeListener,
    AccessibilityManager.TouchExplorationStateChangeListener {
    private val manager = view.context.getSystemService(AccessibilityManager::class.java)

    private class Node(
        val id: Int,
        val parent: Int,
        val bounds: Rect,
        val flags: Int,
        val role: Int,
        val text: String,
        val description: String,
        val editableText: String,
        val state: String,
        val error: String,
        val range: FloatArray?,
        /** A scrolling container's offset, the largest, and its viewport's length. */
        val scroll: FloatArray?,
        /** The scroll actions it offers (`accessibility::scroll_action`'s bits). */
        val scrollActions: Int,
    ) {
        val children = ArrayList<Int>()
    }

    private var nodes = SparseArray<Node>()
    private val roots = ArrayList<Int>()
    private var stale = true
    private var accessibilityFocused = View.NO_ID

    /** The scrolling containers' offsets as last read, for their `TYPE_VIEW_SCROLLED`. */
    private var scrolledTo = SparseArray<Float>()

    /** A change is told to the services at most this often (Compose's bounds updates' 100 ms). */
    private var changePosted = false
    private val changed = Runnable {
        changePosted = false
        tell()
    }

    /**
     * The semantics may have changed (a frame's `SEMANTICS`, the host gone): read again when
     * next asked, whether or not a service is on now (one connecting later reads the current
     * tree, D630). Nothing else is done with no service on, as Compose's `onSemanticsChange`.
     */
    fun semanticsChanged() {
        stale = true
        if (!changePosted && manager.isEnabled) {
            changePosted = true
            view.postDelayed(changed, 100)
        }
    }

    /** The view is attached: services turned on later are told the content. */
    fun attach() {
        manager.addAccessibilityStateChangeListener(this)
        manager.addTouchExplorationStateChangeListener(this)
    }

    fun detach() {
        manager.removeAccessibilityStateChangeListener(this)
        manager.removeTouchExplorationStateChangeListener(this)
        view.removeCallbacks(changed)
        changePosted = false
    }

    override fun onAccessibilityStateChanged(enabled: Boolean) {
        if (enabled) turnedOn()
    }

    override fun onTouchExplorationStateChanged(enabled: Boolean) {
        if (manager.isEnabled) turnedOn()
    }

    /**
     * A service turned on: the tree is read again and the window's content told changed. Scrolls
     * made while none was on aren't told (the offsets are read afresh).
     */
    private fun turnedOn() {
        stale = true
        scrolledTo = SparseArray()
        tell()
    }

    /** Tells the services on that the content changed, and each scroll since last told. */
    private fun tell() {
        if (!manager.isEnabled) return
        val parent = view.parent ?: return
        // A scroll (by an action, a drag, a fling) is told once laid out, with how far, as
        // Compose's `sendScrollEventIfNeeded` (D491).
        refresh()
        val offsets = SparseArray<Float>()
        for (index in 0 until nodes.size()) {
            val node = nodes.valueAt(index)
            val scroll = node.scroll ?: continue
            offsets.put(node.id, scroll[0])
            val before = scrolledTo.get(node.id) ?: continue
            if (before == scroll[0]) continue
            val scrolled = AccessibilityEvent.obtain(AccessibilityEvent.TYPE_VIEW_SCROLLED)
            scrolled.setSource(view, node.id)
            scrolled.packageName = view.context.packageName
            val delta = (scroll[0] - before).toInt()
            if (node.flags and SCROLL_HORIZONTAL != 0) {
                scrolled.scrollX = scroll[0].toInt()
                scrolled.maxScrollX = scroll[1].toInt()
                if (android.os.Build.VERSION.SDK_INT >= 28) scrolled.scrollDeltaX = delta
            } else {
                scrolled.scrollY = scroll[0].toInt()
                scrolled.maxScrollY = scroll[1].toInt()
                if (android.os.Build.VERSION.SDK_INT >= 28) scrolled.scrollDeltaY = delta
            }
            parent.requestSendAccessibilityEvent(view, scrolled)
        }
        scrolledTo = offsets
        val event = AccessibilityEvent.obtain(AccessibilityEvent.TYPE_WINDOW_CONTENT_CHANGED)
        event.contentChangeTypes = AccessibilityEvent.CONTENT_CHANGE_TYPE_SUBTREE
        event.setSource(view)
        parent.requestSendAccessibilityEvent(view, event)
    }

    private fun refresh() {
        if (!stale) return
        stale = false
        val read = SparseArray<Node>()
        roots.clear()
        // No host (destroyed): no nodes.
        val buffer = view.semantics() ?: run {
            nodes = read
            return
        }
        buffer.order(ByteOrder.LITTLE_ENDIAN)
        buffer.position(0)
        while (buffer.hasRemaining()) {
            val id = buffer.getInt()
            val parent = buffer.getInt()
            val bounds = Rect(
                buffer.getFloat().toInt(),
                buffer.getFloat().toInt(),
                buffer.getFloat().toInt(),
                buffer.getFloat().toInt(),
            )
            val flags = buffer.getInt()
            val role = buffer.get().toInt()
            val text = buffer.readString()
            val description = buffer.readString()
            val editableText = buffer.readString()
            val state = buffer.readString()
            val error = buffer.readString()
            val range = if (flags and RANGE != 0) {
                floatArrayOf(buffer.getFloat(), buffer.getFloat(), buffer.getFloat())
            } else {
                null
            }
            val scroll = if (flags and SCROLL != 0) {
                floatArrayOf(buffer.getFloat(), buffer.getFloat(), buffer.getFloat())
            } else {
                null
            }
            val scrollActions = if (flags and SCROLL != 0) buffer.get().toInt() and 0xFF else 0
            val node = Node(
                id, parent, bounds, flags, role, text, description, editableText, state, error, range,
                scroll, scrollActions,
            )
            read.put(id, node)
            if (parent == -1) roots.add(id) else read.get(parent)?.children?.add(id)
        }
        nodes = read
    }

    private fun ByteBuffer.readString(): String {
        val bytes = ByteArray(getInt())
        get(bytes)
        return String(bytes, Charsets.UTF_8)
    }

    override fun createAccessibilityNodeInfo(virtualViewId: Int): AccessibilityNodeInfo? {
        refresh()
        if (virtualViewId == HOST_VIEW_ID) {
            val info = newNodeInfo(view, View.NO_ID)
            view.onInitializeAccessibilityNodeInfo(info)
            for (root in roots) info.addChild(view, root)
            return info
        }
        val node = nodes.get(virtualViewId) ?: return null
        val info = newNodeInfo(view, virtualViewId)
        info.packageName = view.context.packageName
        info.setParent(view, if (node.parent == -1) View.NO_ID else node.parent)
        for (child in node.children) info.addChild(view, child)
        val location = IntArray(2)
        view.getLocationOnScreen(location)
        val screen = Rect(node.bounds)
        screen.offset(location[0], location[1])
        info.setBoundsInScreen(screen)
        info.setBoundsInParent(node.bounds)
        // Clipped away by a scrolling container around it: TalkBack scrolls that instead.
        info.isVisibleToUser = node.flags and HIDDEN == 0
        info.className = when {
            node.flags and EDITABLE != 0 -> "android.widget.EditText"
            node.role == 1 -> "android.widget.Button"
            node.role == 2 -> "android.widget.CheckBox"
            node.role == 3 -> "android.widget.Switch"
            node.role == 4 -> "android.widget.RadioButton"
            node.role == 6 -> "android.widget.ImageView"
            node.role == 7 -> "android.widget.Spinner"
            node.range != null -> "android.widget.SeekBar"
            node.scroll != null && node.flags and SCROLL_HORIZONTAL != 0 -> "android.widget.HorizontalScrollView"
            node.scroll != null -> "android.widget.ScrollView"
            else -> "android.view.View"
        }
        if (node.role == 5) info.extras.putCharSequence("AccessibilityNodeInfo.roleDescription", "Tab")
        if (node.flags and EDITABLE != 0) {
            info.text = node.editableText
            info.isEditable = true
            if (node.text.isNotEmpty()) info.hintText = node.text
        } else if (node.text.isNotEmpty()) {
            info.text = node.text
        }
        if (node.description.isNotEmpty()) info.contentDescription = node.description
        // A state description: `setStateDescription` from API 30; before, in the extras, under
        // the key AndroidX's `AccessibilityNodeInfoCompat.setStateDescription` uses, which
        // accessibility services read back through the compat class, as Compose's.
        if (node.state.isNotEmpty()) {
            if (Build.VERSION.SDK_INT >= 30) {
                info.stateDescription = node.state
            } else {
                info.extras.putCharSequence(STATE_DESCRIPTION_KEY, node.state)
            }
        }
        if (node.error.isNotEmpty()) {
            info.isContentInvalid = true
            info.error = node.error
        }
        info.isPassword = node.flags and PASSWORD != 0
        info.isEnabled = node.flags and DISABLED == 0
        info.isCheckable = node.flags and CHECKABLE != 0
        info.isChecked = node.flags and CHECKED != 0
        info.isSelected = node.flags and SELECTED != 0
        info.isFocusable = node.flags and FOCUSABLE != 0
        info.isFocused = node.flags and FOCUSED != 0
        info.isClickable = node.flags and CLICKABLE != 0
        info.isAccessibilityFocused = accessibilityFocused == virtualViewId
        info.addAction(
            if (accessibilityFocused == virtualViewId) {
                AccessibilityAction.ACTION_CLEAR_ACCESSIBILITY_FOCUS
            } else {
                AccessibilityAction.ACTION_ACCESSIBILITY_FOCUS
            },
        )
        if (info.isEnabled) {
            if (info.isClickable) info.addAction(AccessibilityAction.ACTION_CLICK)
            if (info.isFocusable && !info.isFocused) info.addAction(AccessibilityAction.ACTION_FOCUS)
            node.range?.let { range ->
                info.rangeInfo = newRangeInfo(range[1], range[2], range[0])
                info.addAction(AccessibilityAction.ACTION_SET_PROGRESS)
                info.addAction(AccessibilityAction.ACTION_SCROLL_FORWARD)
                info.addAction(AccessibilityAction.ACTION_SCROLL_BACKWARD)
            }
            // A scrolling container (a lazy list, a scrolling column), as Compose's
            // `ScrollAxisRange` and `ScrollBy`, its actions those Rust works out as Compose's
            // delegate does (`accessibility::scroll_actions`, D491).
            if (node.range == null) node.scroll?.let { scroll ->
                if (scroll[1] > 0f) info.isScrollable = true
                val actions = node.scrollActions
                if (actions and SCROLL_FORWARD != 0) info.addAction(AccessibilityAction.ACTION_SCROLL_FORWARD)
                if (actions and SCROLL_BACKWARD != 0) info.addAction(AccessibilityAction.ACTION_SCROLL_BACKWARD)
                if (actions and SCROLL_UP != 0) info.addAction(AccessibilityAction.ACTION_SCROLL_UP)
                if (actions and SCROLL_DOWN != 0) info.addAction(AccessibilityAction.ACTION_SCROLL_DOWN)
                if (actions and SCROLL_LEFT != 0) info.addAction(AccessibilityAction.ACTION_SCROLL_LEFT)
                if (actions and SCROLL_RIGHT != 0) info.addAction(AccessibilityAction.ACTION_SCROLL_RIGHT)
            }
        }
        if (node.flags and DIALOG != 0) info.paneTitle = node.description.ifEmpty { node.text }
        return info
    }

    override fun performAction(virtualViewId: Int, action: Int, arguments: Bundle?): Boolean {
        if (virtualViewId == HOST_VIEW_ID) return view.performAccessibilityAction(action, arguments)
        refresh()
        val node = nodes.get(virtualViewId) ?: return false
        // As Compose's delegate: the accessibility focus moves to a disabled node, its other
        // actions are refused (`if (!node.enabled()) return false`, D1255).
        if (node.flags and DISABLED != 0 &&
            action != AccessibilityNodeInfo.ACTION_ACCESSIBILITY_FOCUS &&
            action != AccessibilityNodeInfo.ACTION_CLEAR_ACCESSIBILITY_FOCUS
        ) {
            return false
        }
        return when (action) {
            AccessibilityNodeInfo.ACTION_ACCESSIBILITY_FOCUS -> {
                if (accessibilityFocused == virtualViewId) return false
                if (accessibilityFocused != View.NO_ID) {
                    send(accessibilityFocused, AccessibilityEvent.TYPE_VIEW_ACCESSIBILITY_FOCUS_CLEARED)
                }
                accessibilityFocused = virtualViewId
                view.invalidate()
                send(virtualViewId, AccessibilityEvent.TYPE_VIEW_ACCESSIBILITY_FOCUSED)
                true
            }
            AccessibilityNodeInfo.ACTION_CLEAR_ACCESSIBILITY_FOCUS -> {
                if (accessibilityFocused != virtualViewId) return false
                accessibilityFocused = View.NO_ID
                view.invalidate()
                send(virtualViewId, AccessibilityEvent.TYPE_VIEW_ACCESSIBILITY_FOCUS_CLEARED)
                true
            }
            AccessibilityNodeInfo.ACTION_CLICK -> {
                val done = view.performSemanticsAction(virtualViewId, 0, 0f)
                if (done) send(virtualViewId, AccessibilityEvent.TYPE_VIEW_CLICKED)
                done
            }
            AccessibilityNodeInfo.ACTION_FOCUS -> view.performSemanticsAction(virtualViewId, 1, 0f)
            android.R.id.accessibilityActionSetProgress -> {
                val value = arguments?.getFloat(AccessibilityNodeInfo.ACTION_ARGUMENT_PROGRESS_VALUE)
                    ?: return false
                view.performSemanticsAction(virtualViewId, 2, value)
            }
            AccessibilityNodeInfo.ACTION_SCROLL_FORWARD, AccessibilityNodeInfo.ACTION_SCROLL_BACKWARD,
            android.R.id.accessibilityActionScrollDown, android.R.id.accessibilityActionScrollUp,
            android.R.id.accessibilityActionScrollRight, android.R.id.accessibilityActionScrollLeft,
            -> {
                if (node.range == null && node.scroll != null) return scrollPage(node, action)
                if (action != AccessibilityNodeInfo.ACTION_SCROLL_FORWARD &&
                    action != AccessibilityNodeInfo.ACTION_SCROLL_BACKWARD
                ) {
                    return false
                }
                val range = node.range ?: return false
                val step = (range[2] - range[1]) / 20f
                val delta = if (action == AccessibilityNodeInfo.ACTION_SCROLL_FORWARD) step else -step
                view.performSemanticsAction(virtualViewId, 2, (range[0] + delta).coerceIn(range[1], range[2]))
            }
            else -> false
        }
    }

    /**
     * Scrolls a scrolling container by its length along its axis (less nothing: Compose's
     * accessibility scrolls a whole viewport), forward (towards its end) or back; a reversed
     * one's up and down are the other way.
     */
    /**
     * Scrolls a scrolling container a page (its viewport as shown) that way, as Compose's
     * accessibility delegate: Rust works the amount out (`accessibility::scroll_amount`, D491).
     * The services hear it once laid out (`tell`).
     */
    private fun scrollPage(node: Node, action: Int): Boolean {
        if (node.scroll == null) return false
        val direction = when (action) {
            AccessibilityNodeInfo.ACTION_SCROLL_FORWARD -> 3
            AccessibilityNodeInfo.ACTION_SCROLL_BACKWARD -> 4
            android.R.id.accessibilityActionScrollUp -> 5
            android.R.id.accessibilityActionScrollDown -> 6
            android.R.id.accessibilityActionScrollLeft -> 7
            android.R.id.accessibilityActionScrollRight -> 8
            else -> return false
        }
        return view.performSemanticsAction(node.id, direction, 0f)
    }

    private fun send(virtualViewId: Int, type: Int) {
        val parent = view.parent ?: return
        val event = AccessibilityEvent.obtain(type)
        event.setSource(view, virtualViewId)
        event.packageName = view.context.packageName
        parent.requestSendAccessibilityEvent(view, event)
    }

    /** The deepest node under ([x], [y]) in the view, for explore-by-touch. */
    fun nodeAt(x: Float, y: Float): Int {
        refresh()
        var found = View.NO_ID
        fun visit(ids: List<Int>) {
            for (id in ids) {
                val node = nodes.get(id) ?: continue
                if (node.bounds.contains(x.toInt(), y.toInt())) {
                    found = id
                    visit(node.children)
                }
            }
        }
        visit(roots)
        return found
    }

    private var hovered = View.NO_ID

    /** A hover from explore-by-touch: the node under it hears it enter and exit. */
    fun hover(event: MotionEvent): Boolean {
        val id = when (event.actionMasked) {
            MotionEvent.ACTION_HOVER_ENTER, MotionEvent.ACTION_HOVER_MOVE -> nodeAt(event.x, event.y)
            else -> View.NO_ID
        }
        if (id != hovered) {
            if (hovered != View.NO_ID) send(hovered, AccessibilityEvent.TYPE_VIEW_HOVER_EXIT)
            hovered = id
            if (id != View.NO_ID) send(id, AccessibilityEvent.TYPE_VIEW_HOVER_ENTER)
        }
        return id != View.NO_ID
    }

    private companion object {
        /** AndroidX's `AccessibilityNodeInfoCompat.STATE_DESCRIPTION_KEY`, before API 30. */
        const val STATE_DESCRIPTION_KEY =
            "androidx.view.accessibility.AccessibilityNodeInfoCompat.STATE_DESCRIPTION_KEY"
        const val FOCUSABLE = 1 shl 1
        const val FOCUSED = 1 shl 2
        const val EDITABLE = 1 shl 3
        const val PASSWORD = 1 shl 4
        const val DISABLED = 1 shl 5
        const val CHECKABLE = 1 shl 6
        const val CHECKED = 1 shl 7
        const val SELECTED = 1 shl 8
        const val DIALOG = 1 shl 9
        const val RANGE = 1 shl 10
        const val SCROLL = 1 shl 11
        const val SCROLL_HORIZONTAL = 1 shl 12
        const val SCROLL_REVERSED = 1 shl 13
        const val HIDDEN = 1 shl 14
        const val RTL = 1 shl 15
        const val SCROLL_FORWARD = 1
        const val SCROLL_BACKWARD = 1 shl 1
        const val SCROLL_UP = 1 shl 2
        const val SCROLL_DOWN = 1 shl 3
        const val SCROLL_LEFT = 1 shl 4
        const val SCROLL_RIGHT = 1 shl 5
        const val CLICKABLE = 1
    }
}

/**
 * A node's info for [view] (its virtual child [virtualViewId], or the view itself with
 * `View.NO_ID`): constructed from API 30, obtained from the pool before (the constructors are
 * API 30's; lint's NewApi).
 */
private fun newNodeInfo(view: View, virtualViewId: Int): AccessibilityNodeInfo =
    if (Build.VERSION.SDK_INT >= 30) {
        if (virtualViewId == View.NO_ID) AccessibilityNodeInfo(view) else AccessibilityNodeInfo(view, virtualViewId)
    } else {
        @Suppress("DEPRECATION")
        if (virtualViewId == View.NO_ID) AccessibilityNodeInfo.obtain(view) else AccessibilityNodeInfo.obtain(view, virtualViewId)
    }

/** A float range from [min] to [max] at [current]: constructed from API 30, obtained before. */
private fun newRangeInfo(min: Float, max: Float, current: Float): AccessibilityNodeInfo.RangeInfo =
    if (Build.VERSION.SDK_INT >= 30) {
        AccessibilityNodeInfo.RangeInfo(AccessibilityNodeInfo.RangeInfo.RANGE_TYPE_FLOAT, min, max, current)
    } else {
        @Suppress("DEPRECATION")
        AccessibilityNodeInfo.RangeInfo.obtain(AccessibilityNodeInfo.RangeInfo.RANGE_TYPE_FLOAT, min, max, current)
    }
