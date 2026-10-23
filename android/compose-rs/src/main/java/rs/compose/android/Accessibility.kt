package rs.compose.android

import android.graphics.Rect
import android.os.Bundle
import android.util.SparseArray
import android.view.MotionEvent
import android.view.View
import android.view.accessibility.AccessibilityEvent
import android.view.accessibility.AccessibilityNodeInfo
import android.view.accessibility.AccessibilityNodeInfo.AccessibilityAction
import android.view.accessibility.AccessibilityNodeProvider
import java.nio.ByteBuffer
import java.nio.ByteOrder

/**
 * compose-rs's semantics as virtual views (`crates/compose-android/src/accessibility.rs`): what
 * TalkBack reads and acts on. The tree is read from Rust when a service asks after a change.
 */
internal class Accessibility(private val view: ComposeRsView) : AccessibilityNodeProvider() {
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
        /** A scrolling container's offset and the largest, along its axis. */
        val scroll: FloatArray?,
    ) {
        val children = ArrayList<Int>()
    }

    private var nodes = SparseArray<Node>()
    private val roots = ArrayList<Int>()
    private var stale = true
    private var accessibilityFocused = View.NO_ID

    /** The semantics changed: read again when asked, and tell the services. */
    fun invalidate() {
        stale = true
        val parent = view.parent ?: return
        val event = AccessibilityEvent.obtain(AccessibilityEvent.TYPE_WINDOW_CONTENT_CHANGED)
        event.contentChangeTypes = AccessibilityEvent.CONTENT_CHANGE_TYPE_SUBTREE
        event.setSource(view)
        parent.requestSendAccessibilityEvent(view, event)
    }

    private fun refresh() {
        if (!stale) return
        stale = false
        val buffer = view.semantics() ?: return
        buffer.order(ByteOrder.LITTLE_ENDIAN)
        buffer.position(0)
        val read = SparseArray<Node>()
        roots.clear()
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
                floatArrayOf(buffer.getFloat(), buffer.getFloat())
            } else {
                null
            }
            val node = Node(id, parent, bounds, flags, role, text, description, editableText, state, error, range, scroll)
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
            val info = AccessibilityNodeInfo(view)
            view.onInitializeAccessibilityNodeInfo(info)
            for (root in roots) info.addChild(view, root)
            return info
        }
        val node = nodes.get(virtualViewId) ?: return null
        val info = AccessibilityNodeInfo(view, virtualViewId)
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
        if (node.state.isNotEmpty()) info.stateDescription = node.state
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
                info.rangeInfo = AccessibilityNodeInfo.RangeInfo(
                    AccessibilityNodeInfo.RangeInfo.RANGE_TYPE_FLOAT,
                    range[1],
                    range[2],
                    range[0],
                )
                info.addAction(AccessibilityAction.ACTION_SET_PROGRESS)
                info.addAction(AccessibilityAction.ACTION_SCROLL_FORWARD)
                info.addAction(AccessibilityAction.ACTION_SCROLL_BACKWARD)
            }
            // A scrolling container (a lazy list, a scrolling column), as Compose's
            // `ScrollAxisRange` and `ScrollBy`: scrolled a page at a time.
            if (node.range == null) node.scroll?.let { scroll ->
                info.isScrollable = true
                val horizontal = node.flags and SCROLL_HORIZONTAL != 0
                val reversed = node.flags and SCROLL_REVERSED != 0
                if (scroll[0] < scroll[1]) {
                    info.addAction(AccessibilityAction.ACTION_SCROLL_FORWARD)
                    info.addAction(
                        when {
                            horizontal -> AccessibilityAction.ACTION_SCROLL_RIGHT
                            reversed -> AccessibilityAction.ACTION_SCROLL_UP
                            else -> AccessibilityAction.ACTION_SCROLL_DOWN
                        },
                    )
                }
                if (scroll[0] > 0f) {
                    info.addAction(AccessibilityAction.ACTION_SCROLL_BACKWARD)
                    info.addAction(
                        when {
                            horizontal -> AccessibilityAction.ACTION_SCROLL_LEFT
                            reversed -> AccessibilityAction.ACTION_SCROLL_DOWN
                            else -> AccessibilityAction.ACTION_SCROLL_UP
                        },
                    )
                }
            }
        }
        if (node.flags and DIALOG != 0) info.paneTitle = node.description.ifEmpty { node.text }
        return info
    }

    override fun performAction(virtualViewId: Int, action: Int, arguments: Bundle?): Boolean {
        if (virtualViewId == HOST_VIEW_ID) return view.performAccessibilityAction(action, arguments)
        refresh()
        val node = nodes.get(virtualViewId) ?: return false
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
    private fun scrollPage(node: Node, action: Int): Boolean {
        val horizontal = node.flags and SCROLL_HORIZONTAL != 0
        val reversed = node.flags and SCROLL_REVERSED != 0
        val forward = when (action) {
            AccessibilityNodeInfo.ACTION_SCROLL_FORWARD -> true
            AccessibilityNodeInfo.ACTION_SCROLL_BACKWARD -> false
            android.R.id.accessibilityActionScrollDown -> if (horizontal) null else !reversed
            android.R.id.accessibilityActionScrollUp -> if (horizontal) null else reversed
            android.R.id.accessibilityActionScrollRight -> if (horizontal) true else null
            android.R.id.accessibilityActionScrollLeft -> if (horizontal) false else null
            else -> null
        } ?: return false
        val page = if (horizontal) node.bounds.width() else node.bounds.height()
        val delta = if (forward) page.toFloat() else -page.toFloat()
        val done = view.performSemanticsAction(node.id, if (horizontal) 4 else 3, delta)
        if (done) send(node.id, AccessibilityEvent.TYPE_VIEW_SCROLLED)
        return done
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
        const val CLICKABLE = 1
    }
}
