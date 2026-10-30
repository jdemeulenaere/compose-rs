package rs.compose.android

import android.accessibilityservice.AccessibilityServiceInfo
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

    /** The services on, as Compose's: uiautomator isn't among them, and has no events sent. */
    private var enabledServices: List<AccessibilityServiceInfo>? = null

    private val isEnabled: Boolean
        get() {
            if (!manager.isEnabled) return false
            val services = enabledServices ?: manager
                .getEnabledAccessibilityServiceList(AccessibilityServiceInfo.FEEDBACK_ALL_MASK)
                .also { enabledServices = it }
            return services.isNotEmpty()
        }

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
        val paneTitle: String,
        val range: FloatArray?,
        /** A scrolling container's offset, the largest, and its viewport's length. */
        val scroll: FloatArray?,
        /** Who a service reads it before, -1 for none. */
        val before: Int,
        val drawingOrder: Int,
        /** Chosen in Rust, as Compose's delegate does (`accessibility::class_and_role`). */
        val className: String,
        val roleDescription: String?,
        /** The state's words, the library's where the application gives none. */
        val spokenState: String?,
        /** What a service may ask (`accessibility::action`'s codes), with their labels. */
        val actionCodes: IntArray,
        val actionLabels: Array<String>,
    ) {
        val children = ArrayList<Int>()
    }

    private var nodes = SparseArray<Node>()
    private val roots = ArrayList<Int>()
    private var stale = true
    private var accessibilityFocused = View.NO_ID

    /** A service just turned on: nothing it hears comes from a tree before it. */
    private var forgetChanges = false

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
    fun semanticsChanged(atOnce: Boolean) {
        stale = true
        if (!isEnabled) return
        if (atOnce) {
            // A property: said now; where things are can wait (Compose's bounds' batches).
            view.removeCallbacks(changed)
            changePosted = true
            view.post(changed)
        } else if (!changePosted) {
            changePosted = true
            view.postDelayed(changed, 100)
        }
    }

    /** The view is attached: services turned on later are told the content. */
    fun attach() {
        enabledServices = null
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
        enabledServices = null
        if (enabled) turnedOn()
    }

    override fun onTouchExplorationStateChanged(enabled: Boolean) {
        enabledServices = null
        if (manager.isEnabled) turnedOn()
    }

    /**
     * A service turned on: the tree is read again and the window's content told changed. Scrolls
     * made while none was on aren't told (the offsets are read afresh).
     */
    private fun turnedOn() {
        stale = true
        scrolledTo = SparseArray()
        forgetChanges = true
        tell()
        // The content as it is, to read from.
        val parent = view.parent ?: return
        tellChange(parent, -1, CHANGE_SUBTREE, false, "", "", 0, 0, 0)
    }

    /** Tells the services on that the content changed, and each scroll since last told. */
    private fun tell() {
        if (!isEnabled) return
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
        val buffer = view.semanticsChanges(forgetChanges) ?: return
        forgetChanges = false
        buffer.order(ByteOrder.LITTLE_ENDIAN)
        buffer.position(0)
        while (buffer.hasRemaining()) {
            val id = buffer.getInt()
            val kind = buffer.get().toInt()
            val password = buffer.get().toInt() != 0
            val text = buffer.readString()
            val before = buffer.readString()
            val from = buffer.getInt()
            val removed = buffer.getInt()
            val added = buffer.getInt()
            tellChange(parent, id, kind, password, text, before, from, removed, added)
        }
    }

    /** One of `accessibility::ChangeKind`'s, with what Compose's delegate sends for it. */
    @android.annotation.SuppressLint("InlinedApi")
    private fun tellChange(
        parent: android.view.ViewParent,
        id: Int,
        kind: Int,
        password: Boolean,
        text: String,
        before: String,
        from: Int,
        removed: Int,
        added: Int,
    ) {
        val type = when (kind) {
            CHANGE_PANE_APPEARED, CHANGE_PANE_DISAPPEARED, CHANGE_PANE_TITLE, CHANGE_WINDOW ->
                AccessibilityEvent.TYPE_WINDOW_STATE_CHANGED
            CHANGE_TAB_SELECTED -> AccessibilityEvent.TYPE_VIEW_SELECTED
            CHANGE_TEXT_CHANGED -> AccessibilityEvent.TYPE_VIEW_TEXT_CHANGED
            CHANGE_TEXT_SELECTION -> AccessibilityEvent.TYPE_VIEW_TEXT_SELECTION_CHANGED
            CHANGE_FOCUSED -> AccessibilityEvent.TYPE_VIEW_FOCUSED
            else -> AccessibilityEvent.TYPE_WINDOW_CONTENT_CHANGED
        }
        val event = AccessibilityEvent.obtain(type)
        event.packageName = view.context.packageName
        event.isEnabled = true
        // Never read out: a service's echo of a key typed in a password goes by this.
        event.isPassword = password
        event.className = when (kind) {
            CHANGE_TEXT_CHANGED, CHANGE_TEXT_SELECTION -> "android.widget.EditText"
            else -> "android.view.View"
        }
        if (id == -1) event.setSource(view) else event.setSource(view, id)
        when (kind) {
            CHANGE_TEXT_CHANGED -> {
                event.fromIndex = from
                event.removedCount = removed
                event.addedCount = added
                event.beforeText = before
            }
            CHANGE_TEXT_SELECTION -> {
                event.fromIndex = from
                event.toIndex = removed
                event.itemCount = added
            }
            CHANGE_TAB_SELECTED -> if (before.isNotEmpty()) event.contentDescription = before
            else -> event.contentChangeTypes = when (kind) {
                CHANGE_SUBTREE -> AccessibilityEvent.CONTENT_CHANGE_TYPE_SUBTREE
                CHANGE_TEXT -> AccessibilityEvent.CONTENT_CHANGE_TYPE_TEXT
                CHANGE_CONTENT_DESCRIPTION -> AccessibilityEvent.CONTENT_CHANGE_TYPE_CONTENT_DESCRIPTION
                CHANGE_STATE_DESCRIPTION -> AccessibilityEvent.CONTENT_CHANGE_TYPE_STATE_DESCRIPTION
                CHANGE_ERROR ->
                    AccessibilityEvent.CONTENT_CHANGE_TYPE_ERROR or
                        AccessibilityEvent.CONTENT_CHANGE_TYPE_CONTENT_INVALID
                CHANGE_CHECKED -> AccessibilityEvent.CONTENT_CHANGE_TYPE_CHECKED
                CHANGE_PANE_APPEARED -> AccessibilityEvent.CONTENT_CHANGE_TYPE_PANE_APPEARED
                CHANGE_PANE_DISAPPEARED -> AccessibilityEvent.CONTENT_CHANGE_TYPE_PANE_DISAPPEARED
                CHANGE_PANE_TITLE -> AccessibilityEvent.CONTENT_CHANGE_TYPE_PANE_TITLE
                else -> AccessibilityEvent.CONTENT_CHANGE_TYPE_UNDEFINED
            }
        }
        if (text.isNotEmpty()) {
            if (kind == CHANGE_CONTENT_DESCRIPTION) event.contentDescription = text else event.text.add(text)
        }
        parent.requestSendAccessibilityEvent(view, event)
    }

    /** The library's words for `accessibility::spoken`'s [code]. */
    private fun spokenWords(code: Int, percent: Int): String? {
        val resources = view.context.resources
        return when (code) {
            1 -> resources.getString(R.string.compose_rs_state_on)
            2 -> resources.getString(R.string.compose_rs_state_off)
            3 -> resources.getString(R.string.compose_rs_state_partially_checked)
            4 -> resources.getString(R.string.compose_rs_state_selected)
            5 -> resources.getString(R.string.compose_rs_state_not_selected)
            6 -> resources.getString(R.string.compose_rs_state_percent, percent)
            7 -> resources.getString(R.string.compose_rs_state_in_progress)
            8 -> resources.getString(R.string.compose_rs_state_empty)
            9 -> resources.getString(R.string.compose_rs_role_tab)
            10 -> resources.getString(R.string.compose_rs_role_switch)
            else -> null
        }
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
            val paneTitle = buffer.readString()
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
            if (flags and SCROLL != 0) buffer.get()
            val before = buffer.getInt()
            val drawingOrder = buffer.getInt()
            val className = buffer.readString()
            val roleDescription = spokenWords(buffer.get().toInt(), 0)
            val stateCode = buffer.get().toInt()
            val spokenState = spokenWords(stateCode, buffer.get().toInt() and 0xFF)
            val actionCount = buffer.get().toInt() and 0xFF
            val actionCodes = IntArray(actionCount)
            val actionLabels = Array(actionCount) {
                actionCodes[it] = buffer.get().toInt() and 0xFF
                buffer.readString()
            }
            val node = Node(
                id, parent, bounds, flags, role, text, description, editableText, state, error,
                paneTitle, range, scroll, before, drawingOrder, className, roleDescription,
                spokenState, actionCodes, actionLabels,
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
        info.className = node.className
        node.roleDescription?.let {
            info.extras.putCharSequence("AccessibilityNodeInfo.roleDescription", it)
        }
        info.drawingOrder = node.drawingOrder
        info.isImportantForAccessibility = node.flags and IMPORTANT != 0
        info.isLongClickable = node.flags and LONG_CLICKABLE != 0
        if (Build.VERSION.SDK_INT >= 28) {
            info.isScreenReaderFocusable = node.flags and SCREEN_READER_FOCUSABLE != 0
            info.isHeading = node.flags and HEADING != 0
        }
        info.liveRegion = when {
            node.flags and LIVE_ASSERTIVE != 0 -> View.ACCESSIBILITY_LIVE_REGION_ASSERTIVE
            node.flags and LIVE_POLITE != 0 -> View.ACCESSIBILITY_LIVE_REGION_POLITE
            else -> View.ACCESSIBILITY_LIVE_REGION_NONE
        }
        if (node.before != -1) info.setTraversalBefore(view, node.before)
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
        val state = node.spokenState ?: node.state.ifEmpty { null }
        if (state != null) {
            if (Build.VERSION.SDK_INT >= 30) {
                info.stateDescription = state
            } else {
                info.extras.putCharSequence(STATE_DESCRIPTION_KEY, state)
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
        node.range?.let { range -> info.rangeInfo = newRangeInfo(range[1], range[2], range[0]) }
        // A scrolling container (a lazy list, a scrolling column), as Compose's `ScrollAxisRange`.
        if (node.range == null) node.scroll?.let { scroll -> if (scroll[1] > 0f) info.isScrollable = true }
        // Worked out in Rust, as Compose's delegate does (`accessibility::actions_of`, D1289).
        for (index in node.actionCodes.indices) {
            val label = node.actionLabels[index].ifEmpty { null }
            info.addAction(AccessibilityAction(androidAction(node.actionCodes[index]), label))
        }
        if (node.paneTitle.isNotEmpty()) {
            info.paneTitle = node.paneTitle
        } else if (node.flags and DIALOG != 0) {
            info.paneTitle = node.description.ifEmpty { node.text }
        }
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
            else -> {
                val index = node.actionCodes.indexOfFirst { androidAction(it) == action }
                if (index < 0) return false
                val code = node.actionCodes[index]
                val value = if (code == CODE_SET_PROGRESS) {
                    arguments?.getFloat(AccessibilityNodeInfo.ACTION_ARGUMENT_PROGRESS_VALUE)
                        ?: return false
                } else {
                    0f
                }
                val done = view.performSemanticsAction(virtualViewId, code, value)
                if (done && code == CODE_CLICK) send(virtualViewId, AccessibilityEvent.TYPE_VIEW_CLICKED)
                done
            }
        }
    }

    /**
     * The Android action of one of `accessibility::action`'s codes. A custom action's id is any
     * this view's nodes don't otherwise use: a service only hands it back.
     */
    private fun androidAction(code: Int): Int = when (code) {
        CODE_CLICK -> AccessibilityNodeInfo.ACTION_CLICK
        CODE_FOCUS -> AccessibilityNodeInfo.ACTION_FOCUS
        CODE_SET_PROGRESS -> android.R.id.accessibilityActionSetProgress
        CODE_CLEAR_FOCUS -> AccessibilityNodeInfo.ACTION_CLEAR_FOCUS
        3, 17 -> AccessibilityNodeInfo.ACTION_SCROLL_FORWARD
        4, 18 -> AccessibilityNodeInfo.ACTION_SCROLL_BACKWARD
        5 -> android.R.id.accessibilityActionScrollUp
        6 -> android.R.id.accessibilityActionScrollDown
        7 -> android.R.id.accessibilityActionScrollLeft
        8 -> android.R.id.accessibilityActionScrollRight
        9 -> AccessibilityNodeInfo.ACTION_LONG_CLICK
        10 -> AccessibilityNodeInfo.ACTION_EXPAND
        11 -> AccessibilityNodeInfo.ACTION_COLLAPSE
        12 -> AccessibilityNodeInfo.ACTION_DISMISS
        13 -> android.R.id.accessibilityActionPageUp
        14 -> android.R.id.accessibilityActionPageDown
        15 -> android.R.id.accessibilityActionPageLeft
        16 -> android.R.id.accessibilityActionPageRight
        else -> CUSTOM_ACTION_BASE + code
    }

    private fun send(virtualViewId: Int, type: Int) {
        val parent = view.parent ?: return
        val event = AccessibilityEvent.obtain(type)
        event.setSource(view, virtualViewId)
        event.packageName = view.context.packageName
        parent.requestSendAccessibilityEvent(view, event)
    }

    /** The node under ([x], [y]) in the view, for explore-by-touch (`accessibility::node_at`). */
    fun nodeAt(x: Float, y: Float): Int {
        refresh()
        val found = view.semanticsNodeAt(x, y)
        return if (found == -1) View.NO_ID else found
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
        const val HEADING = 1 shl 16
        const val SCREEN_READER_FOCUSABLE = 1 shl 17
        const val LIVE_POLITE = 1 shl 18
        const val LIVE_ASSERTIVE = 1 shl 19
        const val LONG_CLICKABLE = 1 shl 20
        const val IMPORTANT = 1 shl 21
        const val CLICKABLE = 1

        // `accessibility::action`'s, those treated apart here.
        const val CODE_CLICK = 0
        const val CODE_FOCUS = 1
        const val CODE_SET_PROGRESS = 2
        const val CODE_CLEAR_FOCUS = 19

        /** Custom actions' ids: in no package's resources (`0x7f` an application's own). */
        const val CUSTOM_ACTION_BASE = 0x7fff0000

        // `accessibility::ChangeKind`'s.
        const val CHANGE_SUBTREE = 1
        const val CHANGE_TEXT = 2
        const val CHANGE_CONTENT_DESCRIPTION = 3
        const val CHANGE_STATE_DESCRIPTION = 4
        const val CHANGE_ERROR = 5
        const val CHANGE_CHECKED = 6
        const val CHANGE_PANE_APPEARED = 7
        const val CHANGE_PANE_DISAPPEARED = 8
        const val CHANGE_PANE_TITLE = 9
        const val CHANGE_TAB_SELECTED = 10
        const val CHANGE_TEXT_CHANGED = 11
        const val CHANGE_TEXT_SELECTION = 12
        const val CHANGE_FOCUSED = 13
        const val CHANGE_WINDOW = 14
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
