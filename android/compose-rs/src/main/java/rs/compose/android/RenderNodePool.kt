package rs.compose.android

import android.graphics.RenderNode

/**
 * Render nodes kept for reuse (D195): a paragraph's or a group's node, once released, records
 * the next one's drawing instead of waiting for the garbage collector.
 *
 * A dropped `RenderNode` frees its native side, the recording the render thread last drew
 * included, only when its Java object is collected: `discardDisplayList` drops the recording
 * not yet drawn, while the drawn one lives as long as the node (the render thread frees it
 * when it next draws the node, which a node no one draws never is). An app that allocates
 * little on the Java heap is collected rarely, so a scrolled list's released nodes piled up
 * (about 7 MB of native heap after 10 flings through the feed). A reused node's old
 * recording is freed the first time its new one is drawn.
 */
internal class RenderNodePool(private val name: String, private val capacity: Int) {
    private val free = ArrayList<RenderNode>(capacity)

    /** A node with no recording: a released one if any, else a new one. */
    fun obtain(): RenderNode = if (free.isEmpty()) RenderNode(name) else free.removeAt(free.size - 1)

    /**
     * Takes back [node], which nothing recorded or kept from now on draws: its recording is
     * dropped, and it's kept for [obtain] unless the pool is full.
     */
    fun recycle(node: RenderNode) {
        node.discardDisplayList()
        if (free.size < capacity) free.add(node)
    }

    fun clear() {
        free.clear()
    }
}
