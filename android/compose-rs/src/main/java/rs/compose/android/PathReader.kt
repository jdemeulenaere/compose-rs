package rs.compose.android

import android.graphics.Path
import java.nio.ByteBuffer

/** Reads paths as `serialize::write_path_contents` writes them. */
internal class PathReader {
    private val stepPaths = ArrayList<Path>(STEP_POOL_CAPACITY)
    private val stepStack = ArrayList<Path>()

    fun read(buffer: ByteBuffer, path: Path) {
        val fillType = readFillType(buffer)
        val steps = buffer.getInt()
        if (steps > 0) {
            val stack = stepStack
            repeat(steps) {
                when (buffer.get().toInt()) {
                    0 -> {
                        val operand = stepPath()
                        val operandFill = readFillType(buffer)
                        buffer.getInt() // An operand has no steps.
                        readCommands(buffer, operand)
                        operand.fillType = operandFill
                        stack.add(operand)
                    }
                    1 -> {
                        val op = readOp(buffer)
                        val second = stack.removeAt(stack.size - 1)
                        // A failure leaves the first as it was, as `Path.op`.
                        stack[stack.size - 1].op(second, op)
                        releaseStepPath(second)
                    }
                    4 -> {
                        // `path.op(first, second)`: the path's earlier contents if it fails.
                        val op = readOp(buffer)
                        val second = stack.removeAt(stack.size - 1)
                        val first = stack.removeAt(stack.size - 1)
                        // On the stack's top, the result, or as it was if the operation failed.
                        stack[stack.size - 1].op(first, second, op)
                        releaseStepPath(first)
                        releaseStepPath(second)
                    }
                    2 -> {
                        val second = stack.removeAt(stack.size - 1)
                        stack[stack.size - 1].addPath(second)
                        releaseStepPath(second)
                    }
                    else -> stack[stack.size - 1].fillType = readFillType(buffer)
                }
            }
            if (stack.isNotEmpty()) path.set(stack[stack.size - 1])
            while (stack.isNotEmpty()) releaseStepPath(stack.removeAt(stack.size - 1))
        }
        readCommands(buffer, path)
        path.fillType = fillType
    }

    private fun readOp(buffer: ByteBuffer): Path.Op = when (buffer.get().toInt()) {
        0 -> Path.Op.DIFFERENCE
        1 -> Path.Op.INTERSECT
        2 -> Path.Op.UNION
        3 -> Path.Op.XOR
        else -> Path.Op.REVERSE_DIFFERENCE
    }

    private fun readFillType(buffer: ByteBuffer): Path.FillType =
        if (buffer.get().toInt() == 1) Path.FillType.EVEN_ODD else Path.FillType.WINDING

    private fun readCommands(buffer: ByteBuffer, path: Path) {
        repeat(buffer.getInt()) {
            when (buffer.get().toInt()) {
                0 -> path.moveTo(buffer.float(), buffer.float())
                1 -> path.lineTo(buffer.float(), buffer.float())
                2 -> path.quadTo(buffer.float(), buffer.float(), buffer.float(), buffer.float())
                3 -> path.cubicTo(
                    buffer.float(),
                    buffer.float(),
                    buffer.float(),
                    buffer.float(),
                    buffer.float(),
                    buffer.float(),
                )
                else -> path.close()
            }
        }
    }

    /** A scratch path for a path's steps: a pooled one, rewound, or a new one. */
    private fun stepPath(): Path =
        if (stepPaths.isEmpty()) Path() else stepPaths.removeAt(stepPaths.size - 1)

    /** Rewinds [path] and keeps it for the next steps (at most [STEP_POOL_CAPACITY]), as D410's. */
    private fun releaseStepPath(path: Path) {
        path.rewind()
        if (stepPaths.size < STEP_POOL_CAPACITY) stepPaths.add(path)
    }

    private companion object {
        /** How many scratch paths are kept for a path's steps. */
        const val STEP_POOL_CAPACITY = 16
    }
}
