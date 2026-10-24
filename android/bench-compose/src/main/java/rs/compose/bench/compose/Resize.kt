package rs.compose.bench.compose

import androidx.compose.animation.core.LinearEasing
import androidx.compose.animation.core.RepeatMode
import androidx.compose.animation.core.animateFloat
import androidx.compose.animation.core.infiniteRepeatable
import androidx.compose.animation.core.rememberInfiniteTransition
import androidx.compose.animation.core.tween
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Scaffold
import androidx.compose.material3.Text
import androidx.compose.material3.TopAppBar
import androidx.compose.runtime.Composable
import androidx.compose.runtime.key
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.layout.layout
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.Constraints
import androidx.compose.ui.unit.dp
import kotlin.math.roundToInt

/*
 * The resize screen of examples/feed (compose-rs, `resize.rs`), widget for widget: texts in a
 * container whose width animates every frame, for ad hoc checks of text layout across width
 * changes. Not one of measure.py's scenarios.
 *
 * `resize`: a top app bar over a box whose content is laid out at a width animating between half
 * and all of it (1.5 s each way, linear, read while measuring): a title, a wrapping paragraph, a
 * centered title across the whole width, an ellipsized line, a grid of 10 × 4 labelled cells and
 * 12 nested levels of labels.
 */

const val RESIZE_ROWS = 10
const val RESIZE_COLUMNS = 4
const val RESIZE_LEVELS = 12

@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun ResizeScreen() {
    Scaffold(topBar = { TopAppBar(title = { Text("Resize") }) }) { padding ->
        val transition = rememberInfiniteTransition(label = "Resize")
        val fraction = transition.animateFloat(
            0.5f, 1f, infiniteRepeatable(tween(1500, easing = LinearEasing), RepeatMode.Reverse), label = "width",
        )
        Box(
            Modifier.padding(padding).fillMaxSize().layout { measurable, constraints ->
                val width = (constraints.maxWidth * fraction.value).roundToInt()
                val placeable = measurable.measure(Constraints.fixed(width, constraints.maxHeight))
                layout(constraints.maxWidth, constraints.maxHeight) { placeable.place(0, 0) }
            },
        ) {
            ResizeContent()
        }
    }
}

@Composable
private fun ResizeContent() {
    val typography = MaterialTheme.typography
    Column(Modifier.fillMaxSize().padding(8.dp), verticalArrangement = Arrangement.spacedBy(4.dp)) {
        Text("Resizing", style = typography.titleLarge)
        Text("${SENTENCES[0]} ${SENTENCES[1]}", style = typography.bodyMedium)
        Text(
            "Centered across the width",
            modifier = Modifier.fillMaxWidth(),
            textAlign = TextAlign.Center,
            style = typography.titleMedium,
        )
        Text(
            "${SENTENCES[2]} ${SENTENCES[3]}",
            maxLines = 1,
            overflow = TextOverflow.Ellipsis,
            style = typography.bodySmall,
        )
        ResizeGrid()
        ResizeDeep(0)
    }
}

@Composable
private fun ResizeGrid() {
    Column(verticalArrangement = Arrangement.spacedBy(4.dp)) {
        for (row in 0 until RESIZE_ROWS) {
            key(row) {
                Row(Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.spacedBy(4.dp)) {
                    for (column in 0 until RESIZE_COLUMNS) {
                        val index = row * RESIZE_COLUMNS + column
                        Box(
                            Modifier.weight(1f).height(28.dp).background(Color(COLORS[index % 8])),
                            contentAlignment = Alignment.Center,
                        ) {
                            Text(
                                "R${row + 1} C${column + 1}",
                                style = MaterialTheme.typography.labelMedium,
                                color = Color.White,
                            )
                        }
                    }
                }
            }
        }
    }
}

@Composable
private fun ResizeDeep(level: Int) {
    Column(Modifier.fillMaxWidth().padding(start = 6.dp)) {
        Text("Level ${level + 1} · ${TAGS[level % 6]}", style = MaterialTheme.typography.labelLarge)
        if (level + 1 < RESIZE_LEVELS) ResizeDeep(level + 1)
    }
}
