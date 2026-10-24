package rs.compose.bench.compose

import android.graphics.Bitmap
import androidx.compose.animation.animateColor
import androidx.compose.animation.animateContentSize
import androidx.compose.animation.core.LinearEasing
import androidx.compose.animation.core.RepeatMode
import androidx.compose.animation.core.animateFloat
import androidx.compose.animation.core.infiniteRepeatable
import androidx.compose.animation.core.rememberInfiniteTransition
import androidx.compose.animation.core.tween
import androidx.compose.foundation.Canvas
import androidx.compose.foundation.ExperimentalFoundationApi
import androidx.compose.foundation.Image
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.LazyRow
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.ArrowForward
import androidx.compose.material.icons.filled.Build
import androidx.compose.material.icons.filled.Favorite
import androidx.compose.material.icons.filled.MoreVert
import androidx.compose.material.icons.filled.Send
import androidx.compose.material.icons.filled.Share
import androidx.compose.material.icons.filled.Star
import androidx.compose.material.icons.filled.ThumbUp
import androidx.compose.material3.AssistChip
import androidx.compose.material3.Button
import androidx.compose.material3.Card
import androidx.compose.material3.ElevatedCard
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.FilterChip
import androidx.compose.material3.Icon
import androidx.compose.material3.LinearProgressIndicator
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.OutlinedCard
import androidx.compose.material3.Scaffold
import androidx.compose.material3.SuggestionChip
import androidx.compose.material3.Text
import androidx.compose.material3.TopAppBar
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.State
import androidx.compose.runtime.derivedStateOf
import androidx.compose.runtime.getValue
import androidx.compose.runtime.key
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.mutableLongStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.runtime.withFrameMillis
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.draw.drawBehind
import androidx.compose.ui.draw.shadow
import androidx.compose.ui.geometry.CornerRadius
import androidx.compose.ui.graphics.Brush
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.ImageBitmap
import androidx.compose.ui.graphics.Path
import androidx.compose.ui.graphics.asImageBitmap
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.graphics.drawscope.Stroke
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.layout.ContentScale
import androidx.compose.ui.layout.layout
import androidx.compose.ui.text.SpanStyle
import androidx.compose.ui.text.buildAnnotatedString
import androidx.compose.ui.text.font.FontStyle
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextDecoration
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.text.withStyle
import androidx.compose.ui.unit.Constraints
import androidx.compose.ui.unit.dp
import kotlin.math.PI
import kotlin.math.roundToInt
import kotlin.math.sin
import kotlinx.coroutines.delay

/*
 * The stress screens (docs/15): much heavier than the feed, the same as examples/feed's
 * stress.rs (compose-rs), widget for widget.
 *
 * - `stress`: a lazy list of 1,000 items of four kinds, under a sticky header every 20 items:
 *   carousels (a lazy row of image tiles: procedural bitmaps), rich posts (an elevated card with
 *   a gradient avatar, styled text and icons), action cards (chips, buttons and a progress bar)
 *   and banners (a gradient box with a shadow and icons over it).
 * - `stress_animation`: many animations at once: a grid of 140 cells each turning, scaling,
 *   fading and changing color forever (read while drawing), a text changing every frame, bars
 *   whose widths animate (read while measuring) beside percentages (recomposed), a card growing
 *   and shrinking (`animateContentSize`), and a canvas of 50 sine waves drawn anew each frame.
 */

const val STRESS_ITEMS = 1_000
const val SECTION_ITEMS = 20
const val TILES = 12
const val BITMAPS = 8
const val BITMAP_WIDTH = 240
const val BITMAP_HEIGHT = 180
const val GRID_ROWS = 10
const val GRID_COLUMNS = 14
const val BARS = 6
const val WAVES = 50
const val WAVE_SEGMENTS = 48

/**
 * Bitmap [which]'s pixel at ([x], [y]), as 0xAARRGGBB: a diagonal gradient between two of the
 * palette's colors with lighter rings around a point that moves with [which]. Integer arithmetic,
 * as stress.rs's `bitmap_pixel`.
 */
fun stressBitmapPixel(which: Int, x: Int, y: Int): Int {
    val from = COLORS[which % 8]
    val to = COLORS[(which + 3) % 8]
    val span = BITMAP_WIDTH + BITMAP_HEIGHT - 2
    val along = x + y
    val dx = x - (40 + which * 23 % 160)
    val dy = y - (30 + which * 37 % 120)
    val lift = if ((dx * dx + dy * dy) / 96 % 12 < 4) 48 else 0
    fun channel(shift: Int): Int {
        val a = ((from shr shift) and 0xFF).toInt()
        val b = ((to shr shift) and 0xFF).toInt()
        return minOf(255, (a * (span - along) + b * along) / span + lift)
    }
    return (0xFF shl 24) or (channel(16) shl 16) or (channel(8) shl 8) or channel(0)
}

fun stressBitmap(which: Int): ImageBitmap {
    val pixels = IntArray(BITMAP_WIDTH * BITMAP_HEIGHT)
    for (y in 0 until BITMAP_HEIGHT) {
        for (x in 0 until BITMAP_WIDTH) {
            pixels[y * BITMAP_WIDTH + x] = stressBitmapPixel(which, x, y)
        }
    }
    return Bitmap.createBitmap(pixels, BITMAP_WIDTH, BITMAP_HEIGHT, Bitmap.Config.ARGB_8888).asImageBitmap()
}

/** The bitmaps, made the first time a tile asks (as stress.rs's thread-local). */
private val STRESS_BITMAPS: List<ImageBitmap> by lazy { List(BITMAPS) { stressBitmap(it) } }

private fun paletteColor(index: Int) = Color(COLORS[index % 8])

/** What an item of the stress list is, from its index; also its `contentType`. */
private fun stressKind(index: Int) = index % 4

@OptIn(ExperimentalMaterial3Api::class, ExperimentalFoundationApi::class)
@Composable
fun StressScroll() {
    Scaffold(topBar = { TopAppBar(title = { Text("Stress") }) }) { padding ->
        // Inside the bars (padding, not contentPadding), as compose-rs's list: the same viewport,
        // and the headers stick below the top bar rather than under it.
        LazyColumn(Modifier.padding(padding).fillMaxSize()) {
            for (section in 0 until STRESS_ITEMS / SECTION_ITEMS) {
                stickyHeader(key = "h$section", contentType = "header") { SectionHeader(section) }
                val first = section * SECTION_ITEMS
                items(
                    SECTION_ITEMS,
                    key = { first + it },
                    contentType = { stressKind(first + it) },
                ) { offset -> StressItem(first + offset) }
            }
        }
    }
}

@Composable
private fun SectionHeader(section: Int) {
    val scheme = MaterialTheme.colorScheme
    Box(
        Modifier.fillMaxWidth()
            .background(scheme.secondaryContainer)
            .padding(horizontal = 16.dp, vertical = 8.dp),
    ) {
        Text(
            "Section ${section + 1}",
            style = MaterialTheme.typography.titleSmall,
            color = scheme.onSecondaryContainer,
        )
    }
}

@Composable
private fun StressItem(index: Int) {
    when (stressKind(index)) {
        0 -> Carousel(index)
        1 -> RichPost(index)
        2 -> ActionCard(index)
        else -> Banner(index)
    }
}

@Composable
private fun Carousel(index: Int) {
    Column(Modifier.fillMaxWidth().padding(vertical = 8.dp)) {
        Row(
            Modifier.fillMaxWidth().padding(horizontal = 16.dp, vertical = 4.dp),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            Text("Collection $index", style = MaterialTheme.typography.titleMedium, modifier = Modifier.weight(1f))
            Icon(Icons.Filled.ArrowForward, contentDescription = "More")
        }
        LazyRow(
            contentPadding = PaddingValues(horizontal = 16.dp),
            horizontalArrangement = Arrangement.spacedBy(8.dp),
        ) {
            items(TILES, key = { it }) { tile -> Tile(index, tile) }
        }
    }
}

@Composable
private fun Tile(index: Int, tile: Int) {
    Column(Modifier.width(120.dp)) {
        Image(
            STRESS_BITMAPS[(index + tile) % BITMAPS],
            contentDescription = null,
            modifier = Modifier.size(120.dp, 90.dp).clip(RoundedCornerShape(12.dp)),
            contentScale = ContentScale.Crop,
        )
        Text(
            "Tile ${tile + 1}",
            style = MaterialTheme.typography.bodySmall,
            maxLines = 1,
            modifier = Modifier.padding(vertical = 4.dp),
        )
    }
}

@Composable
private fun RichPost(index: Int) {
    val author = AUTHORS[index % AUTHORS.size]
    val scheme = MaterialTheme.colorScheme
    val typography = MaterialTheme.typography
    val body = remember(index) {
        buildAnnotatedString {
            withStyle(SpanStyle(fontWeight = FontWeight.Bold)) { append(author.substringBefore(' ')) }
            append(" wrote: ")
            withStyle(SpanStyle(fontStyle = FontStyle.Italic)) { append(SENTENCES[index % SENTENCES.size]) }
            append(" ")
            withStyle(SpanStyle(color = scheme.primary, fontWeight = FontWeight.Medium)) {
                append("#")
                append(TAGS[index % TAGS.size])
            }
            append(" ")
            withStyle(SpanStyle(color = scheme.tertiary, textDecoration = TextDecoration.Underline)) {
                append("Read more")
            }
            append(" ")
            withStyle(SpanStyle(background = Color(0x55FFC107))) {
                append(SENTENCES[(index * 3 + 1) % SENTENCES.size])
            }
        }
    }
    ElevatedCard(Modifier.fillMaxWidth().padding(horizontal = 16.dp, vertical = 6.dp)) {
        Column(Modifier.padding(16.dp)) {
            Row(verticalAlignment = Alignment.CenterVertically) {
                Box(
                    Modifier.size(44.dp)
                        .clip(CircleShape)
                        .background(Brush.linearGradient(listOf(paletteColor(index), paletteColor(index + 3)))),
                    contentAlignment = Alignment.Center,
                ) {
                    Text("${index % 100}", color = Color.White, style = typography.titleSmall)
                }
                Spacer(Modifier.width(12.dp))
                Column(Modifier.weight(1f)) {
                    Text(author, style = typography.titleSmall)
                    Text("${(index * 7) % 24 + 1} h ago", style = typography.bodySmall, color = scheme.onSurfaceVariant)
                }
                Icon(Icons.Filled.MoreVert, contentDescription = "More")
            }
            Spacer(Modifier.height(8.dp))
            Text(body, style = typography.bodyMedium, maxLines = 4, overflow = TextOverflow.Ellipsis)
            Spacer(Modifier.height(8.dp))
            Row(
                horizontalArrangement = Arrangement.spacedBy(16.dp),
                verticalAlignment = Alignment.CenterVertically,
            ) {
                Counted(Icons.Filled.ThumbUp, (index * 37) % 500)
                Counted(Icons.Filled.Share, (index * 11) % 90)
                Counted(Icons.Filled.Star, (index * 5) % 40)
                Counted(Icons.Filled.Send, (index * 3) % 20)
            }
        }
    }
}

@Composable
private fun Counted(icon: ImageVector, count: Int) {
    Row(verticalAlignment = Alignment.CenterVertically) {
        Icon(
            icon,
            contentDescription = null,
            modifier = Modifier.size(18.dp),
            tint = MaterialTheme.colorScheme.onSurfaceVariant,
        )
        Spacer(Modifier.width(4.dp))
        Text("$count", style = MaterialTheme.typography.labelMedium)
    }
}

@Composable
private fun ActionCard(index: Int) {
    val progress = (index * 13 % 100) / 100f
    val typography = MaterialTheme.typography
    OutlinedCard(Modifier.fillMaxWidth().padding(horizontal = 16.dp, vertical = 6.dp)) {
        Column(Modifier.padding(16.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
            Text("Task $index", style = typography.titleSmall)
            Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                AssistChip(
                    onClick = {},
                    label = { Text("Assist") },
                    leadingIcon = { Icon(Icons.Filled.Build, contentDescription = null, modifier = Modifier.size(18.dp)) },
                )
                FilterChip(selected = index % 3 == 0, onClick = {}, label = { Text("Filter") })
                SuggestionChip(onClick = {}, label = { Text("Hint") })
            }
            // Without Material 3 1.4's gap, as stress.rs's, and its stop dot, which compose-rs's
            // indicator doesn't draw: a line, then the track, on both sides.
            LinearProgressIndicator(
                progress = { progress },
                modifier = Modifier.fillMaxWidth(),
                gapSize = 0.dp,
                drawStopIndicator = {},
            )
            Row(
                horizontalArrangement = Arrangement.spacedBy(8.dp),
                verticalAlignment = Alignment.CenterVertically,
            ) {
                Button(onClick = {}) { Text("Accept") }
                OutlinedButton(onClick = {}) { Text("Later") }
                Spacer(Modifier.weight(1f))
                Text("${index * 13 % 100} %", style = typography.labelLarge)
            }
        }
    }
}

@Composable
private fun Banner(index: Int) {
    val shape = RoundedCornerShape(16.dp)
    val typography = MaterialTheme.typography
    Box(
        Modifier.fillMaxWidth()
            .padding(horizontal = 16.dp, vertical = 6.dp)
            .height(120.dp)
            .shadow(6.dp, shape, clip = true)
            .background(
                Brush.linearGradient(listOf(paletteColor(index), paletteColor(index + 2), paletteColor(index + 5))),
            ),
    ) {
        Column(Modifier.align(Alignment.BottomStart).padding(16.dp)) {
            Text("Featured $index", style = typography.titleLarge, color = Color.White)
            Text(
                SENTENCES[(index * 5) % SENTENCES.size],
                style = typography.bodyMedium,
                color = Color.White,
                maxLines = 1,
                overflow = TextOverflow.Ellipsis,
            )
        }
        Row(
            Modifier.align(Alignment.TopEnd).padding(12.dp),
            horizontalArrangement = Arrangement.spacedBy(8.dp),
        ) {
            Icon(Icons.Filled.Favorite, contentDescription = null, tint = Color.White)
            Icon(Icons.Filled.Share, contentDescription = null, tint = Color.White)
            Icon(Icons.Filled.Star, contentDescription = null, tint = Color.White)
        }
    }
}

@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun StressAnimation() {
    Scaffold(topBar = { TopAppBar(title = { Text("Stress animation") }) }) { padding ->
        Column(
            Modifier.padding(padding).fillMaxSize().padding(16.dp),
            verticalArrangement = Arrangement.spacedBy(12.dp),
        ) {
            Ticker()
            Grid()
            Bars()
            ExpandingCard()
            Waves()
        }
    }
}

/** A text changing every frame: the frames since it started, and the milliseconds. */
@Composable
private fun Ticker() {
    var frames by remember { mutableIntStateOf(0) }
    var millis by remember { mutableLongStateOf(0L) }
    LaunchedEffect(Unit) {
        val start = withFrameMillis { it }
        while (true) {
            withFrameMillis { time ->
                frames++
                millis = time - start
            }
        }
    }
    Text("Frame $frames · $millis ms", style = MaterialTheme.typography.titleMedium)
}

@Composable
private fun Grid() {
    Column(verticalArrangement = Arrangement.spacedBy(6.dp)) {
        for (row in 0 until GRID_ROWS) {
            Row(Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.SpaceBetween) {
                for (column in 0 until GRID_COLUMNS) {
                    val index = row * GRID_COLUMNS + column
                    key(index) { Cell(index) }
                }
            }
        }
    }
}

/** A cell turning, scaling, fading and changing color forever, at its own pace; read while drawing. */
@Composable
private fun Cell(index: Int) {
    val transition = rememberInfiniteTransition(label = "Cell")
    val rotation = transition.animateFloat(
        0f, 360f,
        infiniteRepeatable(tween(1500 + index % 5 * 250, easing = LinearEasing), RepeatMode.Restart),
        label = "rotation",
    )
    val scale = transition.animateFloat(
        0.6f, 1f, infiniteRepeatable(tween(700 + index % 7 * 100), RepeatMode.Reverse), label = "scale",
    )
    val alpha = transition.animateFloat(
        0.35f, 1f, infiniteRepeatable(tween(900 + index % 3 * 300), RepeatMode.Reverse), label = "alpha",
    )
    val fill = transition.animateColor(
        paletteColor(index), paletteColor(index + 3),
        infiniteRepeatable(tween(1200 + index % 4 * 200), RepeatMode.Reverse), label = "color",
    )
    Box(
        Modifier.size(22.dp)
            .graphicsLayer {
                rotationZ = rotation.value
                scaleX = scale.value
                scaleY = scale.value
                this.alpha = alpha.value
            }
            .drawBehind {
                val radius = 5.dp.toPx()
                drawRoundRect(fill.value, cornerRadius = CornerRadius(radius, radius))
            },
    )
}

@Composable
private fun Bars() {
    Column(verticalArrangement = Arrangement.spacedBy(6.dp)) {
        for (bar in 0 until BARS) {
            key(bar) { Bar(bar) }
        }
    }
}

/** A bar whose width animates, read while measuring, beside its percentage, recomposed when it changes. */
@Composable
private fun Bar(index: Int) {
    val transition = rememberInfiniteTransition(label = "Bar")
    val fraction = transition.animateFloat(
        0.1f, 1f, infiniteRepeatable(tween(1100 + index * 170), RepeatMode.Reverse), label = "fraction",
    )
    val fill = paletteColor(index)
    Row(verticalAlignment = Alignment.CenterVertically) {
        Percent(fraction)
        Box(Modifier.weight(1f).height(10.dp)) {
            Box(
                Modifier
                    .layout { measurable, constraints ->
                        val width = (constraints.maxWidth * fraction.value).roundToInt()
                        val placeable = measurable.measure(Constraints.fixed(width, constraints.maxHeight))
                        layout(width, constraints.maxHeight) { placeable.place(0, 0) }
                    }
                    .clip(RoundedCornerShape(5.dp))
                    .background(fill),
            )
        }
    }
}

@Composable
private fun Percent(fraction: State<Float>) {
    val percent by remember { derivedStateOf { (fraction.value * 100).roundToInt() } }
    Text("$percent %", modifier = Modifier.width(48.dp), style = MaterialTheme.typography.labelLarge)
}

/** A card whose text grows to four lines and shrinks back to one every second, its size animated. */
@Composable
private fun ExpandingCard() {
    var expanded by remember { mutableStateOf(false) }
    LaunchedEffect(Unit) {
        while (true) {
            delay(1000)
            expanded = !expanded
        }
    }
    Card(Modifier.fillMaxWidth()) {
        Column(Modifier.animateContentSize(tween(500)).padding(12.dp)) {
            Text("Release notes", style = MaterialTheme.typography.titleSmall)
            if (expanded) {
                Text(
                    "${SENTENCES[0]} ${SENTENCES[1]} ${SENTENCES[2]} ${SENTENCES[3]}",
                    style = MaterialTheme.typography.bodyMedium,
                    maxLines = 4,
                    overflow = TextOverflow.Ellipsis,
                )
            } else {
                Text(
                    SENTENCES[0],
                    style = MaterialTheme.typography.bodyMedium,
                    maxLines = 1,
                    overflow = TextOverflow.Ellipsis,
                )
            }
        }
    }
}

/** 50 sine waves, each with its own amplitude, frequency and speed, drawn anew each frame. */
@Composable
private fun Waves() {
    val transition = rememberInfiniteTransition(label = "Waves")
    val time = transition.animateFloat(
        0f, 1f, infiniteRepeatable(tween(4000, easing = LinearEasing), RepeatMode.Restart), label = "time",
    )
    // One path, reset for each wave: the recording keeps a copy of each.
    val path = remember { Path() }
    Canvas(Modifier.fillMaxWidth().height(140.dp)) {
        val t = time.value
        val stroke = Stroke(width = 1.5.dp.toPx())
        for (line in 0 until WAVES) {
            val amplitude = size.height * 0.45f * (0.3f + 0.07f * (line % 10))
            val cycles = 1f + (line % 5) * 0.5f
            val phase = 2f * PI.toFloat() * t * (1 + line % 4) + line * 0.37f
            path.reset()
            for (point in 0..WAVE_SEGMENTS) {
                val along = point.toFloat() / WAVE_SEGMENTS
                val x = size.width * along
                val y = size.height / 2f + amplitude * sin(2f * PI.toFloat() * cycles * along + phase)
                if (point == 0) path.moveTo(x, y) else path.lineTo(x, y)
            }
            drawPath(path, paletteColor(line).copy(alpha = 0.6f), style = stroke)
        }
    }
}
