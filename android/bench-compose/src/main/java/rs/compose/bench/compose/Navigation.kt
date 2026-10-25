package rs.compose.bench.compose

import android.os.Trace
import androidx.activity.compose.BackHandler
import androidx.compose.foundation.Canvas
import androidx.compose.foundation.Image
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.verticalScroll
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.ArrowBack
import androidx.compose.material.icons.filled.ArrowForward
import androidx.compose.material.icons.filled.Favorite
import androidx.compose.material.icons.filled.Place
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
import androidx.compose.material3.IconButton
import androidx.compose.material3.LinearProgressIndicator
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.OutlinedCard
import androidx.compose.material3.Scaffold
import androidx.compose.material3.SuggestionChip
import androidx.compose.material3.Text
import androidx.compose.material3.TopAppBar
import androidx.compose.runtime.Composable
import androidx.compose.runtime.Immutable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.key
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.draw.shadow
import androidx.compose.ui.geometry.CornerRadius
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.Size
import androidx.compose.ui.graphics.Brush
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.Path
import androidx.compose.ui.graphics.drawscope.Stroke
import androidx.compose.ui.layout.ContentScale
import androidx.compose.ui.text.SpanStyle
import androidx.compose.ui.text.buildAnnotatedString
import androidx.compose.ui.text.font.FontStyle
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextDecoration
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.text.withStyle
import androidx.compose.ui.unit.dp

/*
 * The navigation screens (docs/17): the latency of navigating to an expensive screen never seen
 * before, the same as examples/feed's navigation.rs (compose-rs), widget for widget.
 *
 * - `navigation`: a light launcher, a top app bar over one button. Tapping it switches to a
 *   heavy screen (a plain `if`: no NavHost, no transition), whose first frame composes, lays out
 *   and draws NAVIGATION_ITEMS varied items at once in a scrolling column: profile cards,
 *   articles, stats (a canvas bar chart), galleries (images) and banners (a shadow, a gradient,
 *   a canvas sparkline). Back (the key, or the top bar's button) returns to the launcher.
 * - Each visit shows new content: the visit's number (`seed`) drives every string, count and
 *   color, its words made of syllables drawn from it (`NavigationItem.of`, as navigation.rs's),
 *   so no text, paragraph or composition cache from an earlier visit helps either app.
 * - The heavy screen's composition starts with an empty trace section, `navigation: heavy
 *   screen`, which measure.py's `navigation` scenario finds the first frame by.
 */

const val NAVIGATION_ITEMS = 150
const val NAVIGATION_KINDS = 5
const val CHART_BARS = 12
const val SPARKLINE_POINTS = 24

private val SYLLABLES = listOf(
    "ka", "lo", "mi", "ra", "ten", "vu", "sel", "dor", "ni", "pa", "qui", "zen", "bo", "fa", "lin",
    "mur", "os", "ta", "ve", "xi", "ro", "sa", "nel", "dri",
)

/** A 32-bit integer hash (lowbias32), as navigation.rs's `mix`. */
fun mix(value: Int): Int {
    var x = value
    x = x xor (x ushr 16)
    x *= 0x7FEB352D
    x = x xor (x ushr 15)
    x *= 0x846CA68B.toInt()
    x = x xor (x ushr 16)
    return x
}

/** The [salt]th number of item [index] on visit [seed], below [bound], as navigation.rs's. */
fun pick(seed: Int, index: Int, salt: Int, bound: Int): Int {
    val hash = mix(mix((seed * 0x9E3779B9.toInt()) xor index) + salt * 0x85EBCA6B.toInt())
    return (hash ushr 1) % bound
}

/** An item's numbers, drawn in turn, as navigation.rs's `Draws`, in the same order. */
private class Draws(private val seed: Int, private val index: Int) {
    private var salt = 0

    fun below(bound: Int): Int {
        salt += 1
        return pick(seed, index, salt, bound)
    }

    fun word(): String {
        val syllables = 2 + below(3)
        val word = StringBuilder()
        repeat(syllables) { word.append(SYLLABLES[below(SYLLABLES.size)]) }
        return word.toString()
    }

    fun capitalized(): String = word().replaceFirstChar { it.uppercaseChar() }

    fun phrase(count: Int): String {
        val phrase = StringBuilder(capitalized())
        repeat(count - 1) { phrase.append(' ').append(word()) }
        return phrase.toString()
    }

    fun sentence(): String {
        val count = 6 + below(7)
        return phrase(count) + "."
    }

    fun paragraph(count: Int): String {
        val paragraph = StringBuilder(sentence())
        repeat(count - 1) { paragraph.append(' ').append(sentence()) }
        return paragraph.toString()
    }

    fun color(): Long = COLORS[below(COLORS.size)]
}

/** An item's content on visit `seed`, as navigation.rs's `NavigationItem`. */
sealed interface NavigationItem {
    @Immutable
    data class Profile(
        val name: String,
        val initials: String,
        val handle: String,
        val followers: Int,
        val bio: String,
        val chips: List<String>,
        val selected: Boolean,
        val liked: Boolean,
        val colors: List<Long>,
    ) : NavigationItem

    @Immutable
    data class Article(
        val title: String,
        val author: String,
        val minutes: Int,
        val lead: String,
        val quote: String,
        val tag: String,
        val highlight: String,
        val counts: List<Int>,
    ) : NavigationItem

    @Immutable
    data class Stats(
        val label: String,
        val value: Int,
        val delta: Int,
        val up: Boolean,
        val bars: List<Int>,
        val progress: Int,
        val color: Long,
    ) : NavigationItem

    @Immutable
    data class Gallery(val album: String, val images: List<Int>, val caption: String) : NavigationItem

    @Immutable
    data class Banner(
        val title: String,
        val line: String,
        val points: List<Int>,
        val colors: List<Long>,
    ) : NavigationItem

    companion object {
        fun of(seed: Int, index: Int): NavigationItem {
            val draws = Draws(seed, index)
            return when (index % NAVIGATION_KINDS) {
                0 -> {
                    val first = draws.capitalized()
                    val last = draws.capitalized()
                    val handle = "@" + draws.word()
                    val followers = draws.below(5_000)
                    val sentences = 2 + draws.below(2)
                    val bio = draws.paragraph(sentences)
                    val chips = listOf(draws.capitalized(), draws.capitalized(), draws.capitalized())
                    val selected = draws.below(2) == 1
                    val liked = draws.below(2) == 1
                    val colors = listOf(draws.color(), draws.color())
                    Profile("$first $last", "${first[0]}${last[0]}", handle, followers, bio, chips, selected, liked, colors)
                }
                1 -> {
                    val words = 3 + draws.below(4)
                    val title = draws.phrase(words)
                    val author = draws.phrase(2)
                    val minutes = 2 + draws.below(15)
                    val lead = draws.phrase(2)
                    val quote = draws.sentence()
                    val tag = draws.word()
                    val highlight = draws.sentence()
                    val counts = List(4) { draws.below(1_000) }
                    Article(title, author, minutes, lead, quote, tag, highlight, counts)
                }
                2 -> {
                    val label = draws.phrase(2)
                    val value = draws.below(100_000)
                    val delta = draws.below(2_000)
                    val up = draws.below(2) == 1
                    val bars = List(CHART_BARS) { 5 + draws.below(96) }
                    val progress = draws.below(101)
                    val color = draws.color()
                    Stats(label, value, delta, up, bars, progress, color)
                }
                3 -> {
                    val album = "Album " + draws.capitalized()
                    val images = List(3) { draws.below(BITMAPS) }
                    val caption = draws.sentence()
                    Gallery(album, images, caption)
                }
                else -> {
                    val title = draws.phrase(2)
                    val line = draws.sentence()
                    val points = List(SPARKLINE_POINTS) { draws.below(101) }
                    val colors = List(3) { draws.color() }
                    Banner(title, line, points, colors)
                }
            }
        }
    }
}

/** The navigation app: the launcher, or the heavy screen of the visit `seed` while it shows. */
@Composable
fun NavigationApp() {
    var seed by remember { mutableIntStateOf(0) }
    var showing by remember { mutableStateOf(false) }
    if (showing) {
        BackHandler { showing = false }
        HeavyScreen(seed, onBack = { showing = false })
    } else {
        Launcher(seed, onOpen = {
            seed += 1
            showing = true
        })
    }
}

/** A top app bar over one big button, centered: measure.py taps the screen's center. */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
private fun Launcher(visits: Int, onOpen: () -> Unit) {
    val typography = MaterialTheme.typography
    Scaffold(topBar = { TopAppBar(title = { Text("Navigation") }) }) { padding ->
        Box(Modifier.padding(padding).fillMaxSize(), contentAlignment = Alignment.Center) {
            Button(onClick = onOpen, modifier = Modifier.size(320.dp, 240.dp)) {
                Column(horizontalAlignment = Alignment.CenterHorizontally) {
                    Text("Open screen ${visits + 1}", style = typography.titleLarge)
                    Spacer(Modifier.height(8.dp))
                    Text("$visits visits", style = typography.bodyMedium)
                }
            }
        }
    }
}

@OptIn(ExperimentalMaterial3Api::class)
@Composable
private fun HeavyScreen(seed: Int, onBack: () -> Unit) {
    // An empty section, where the heavy screen's first composition starts: measure.py finds the
    // frame drawing it by it.
    Trace.beginSection("navigation: heavy screen")
    Trace.endSection()
    Scaffold(
        topBar = {
            TopAppBar(
                title = { Text("Screen $seed") },
                navigationIcon = {
                    IconButton(onClick = onBack) { Icon(Icons.Filled.ArrowBack, contentDescription = "Back") }
                },
            )
        },
    ) { padding ->
        Column(Modifier.padding(padding).fillMaxSize().verticalScroll(rememberScrollState())) {
            for (index in 0 until NAVIGATION_ITEMS) {
                key(index) { HeavyItem(seed, index) }
            }
        }
    }
}

@Composable
private fun HeavyItem(seed: Int, index: Int) {
    when (val item = remember(seed, index) { NavigationItem.of(seed, index) }) {
        is NavigationItem.Profile -> Profile(item)
        is NavigationItem.Article -> Article(item)
        is NavigationItem.Stats -> Stats(item)
        is NavigationItem.Gallery -> Gallery(item)
        is NavigationItem.Banner -> Banner(item)
    }
}

@Composable
private fun Profile(item: NavigationItem.Profile) {
    val scheme = MaterialTheme.colorScheme
    val typography = MaterialTheme.typography
    ElevatedCard(Modifier.fillMaxWidth().padding(horizontal = 16.dp, vertical = 6.dp)) {
        Column(Modifier.padding(16.dp)) {
            Row(verticalAlignment = Alignment.CenterVertically) {
                Box(
                    Modifier.size(48.dp)
                        .clip(CircleShape)
                        .background(Brush.linearGradient(listOf(Color(item.colors[0]), Color(item.colors[1])))),
                    contentAlignment = Alignment.Center,
                ) {
                    Text(item.initials, color = Color.White, style = typography.titleMedium)
                }
                Spacer(Modifier.width(12.dp))
                Column(Modifier.weight(1f)) {
                    Text(item.name, style = typography.titleMedium)
                    Text(
                        "${item.handle} · ${item.followers} followers",
                        style = typography.bodySmall,
                        color = scheme.onSurfaceVariant,
                    )
                }
                IconButton(onClick = {}) {
                    Icon(
                        Icons.Filled.Favorite,
                        contentDescription = "Like",
                        tint = if (item.liked) Color(0xFFE53935) else scheme.outline,
                    )
                }
            }
            Spacer(Modifier.height(8.dp))
            Text(item.bio, style = typography.bodyMedium)
            Spacer(Modifier.height(8.dp))
            Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                AssistChip(
                    onClick = {},
                    label = { Text(item.chips[0]) },
                    leadingIcon = { Icon(Icons.Filled.Place, contentDescription = null, modifier = Modifier.size(18.dp)) },
                )
                FilterChip(selected = item.selected, onClick = {}, label = { Text(item.chips[1]) })
                SuggestionChip(onClick = {}, label = { Text(item.chips[2]) })
            }
        }
    }
}

@Composable
private fun Article(item: NavigationItem.Article) {
    val scheme = MaterialTheme.colorScheme
    val typography = MaterialTheme.typography
    val body = remember(item) {
        buildAnnotatedString {
            withStyle(SpanStyle(fontWeight = FontWeight.Bold)) { append(item.lead) }
            append(" says: ")
            withStyle(SpanStyle(fontStyle = FontStyle.Italic)) { append(item.quote) }
            append(" ")
            withStyle(SpanStyle(color = scheme.primary, fontWeight = FontWeight.Medium)) {
                append("#")
                append(item.tag)
            }
            append(" ")
            withStyle(SpanStyle(color = scheme.tertiary, textDecoration = TextDecoration.Underline)) {
                append("Read more")
            }
            append(" ")
            withStyle(SpanStyle(background = Color(0x55FFC107))) { append(item.highlight) }
        }
    }
    Card(Modifier.fillMaxWidth().padding(horizontal = 16.dp, vertical = 6.dp)) {
        Column(Modifier.padding(16.dp)) {
            Text(item.title, style = typography.titleLarge)
            Spacer(Modifier.height(4.dp))
            Text(
                "${item.author} · ${item.minutes} min read",
                style = typography.labelMedium,
                color = scheme.onSurfaceVariant,
            )
            Spacer(Modifier.height(8.dp))
            Text(body, style = typography.bodyMedium)
            Spacer(Modifier.height(12.dp))
            Row(
                horizontalArrangement = Arrangement.spacedBy(16.dp),
                verticalAlignment = Alignment.CenterVertically,
            ) {
                Counted(Icons.Filled.ThumbUp, item.counts[0])
                Counted(Icons.Filled.Share, item.counts[1])
                Counted(Icons.Filled.Star, item.counts[2])
                Counted(Icons.Filled.Send, item.counts[3])
            }
        }
    }
}

@Composable
private fun Stats(item: NavigationItem.Stats) {
    val scheme = MaterialTheme.colorScheme
    val typography = MaterialTheme.typography
    val fill = Color(item.color)
    OutlinedCard(Modifier.fillMaxWidth().padding(horizontal = 16.dp, vertical = 6.dp)) {
        Column(Modifier.padding(16.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
            Row(verticalAlignment = Alignment.CenterVertically) {
                Column(Modifier.weight(1f)) {
                    Text(item.label, style = typography.labelLarge)
                    Text("${item.value}", style = typography.headlineSmall)
                    Text(
                        "${if (item.up) "+" else "-"}${item.delta / 10}.${item.delta % 10} %",
                        style = typography.labelMedium,
                        color = if (item.up) scheme.primary else scheme.error,
                    )
                }
                Canvas(Modifier.size(132.dp, 56.dp)) {
                    val gap = 3.dp.toPx()
                    val width = (size.width - gap * (CHART_BARS - 1)) / CHART_BARS
                    val radius = 2.dp.toPx()
                    item.bars.forEachIndexed { bar, percent ->
                        val height = size.height * percent / 100f
                        val left = bar * (width + gap)
                        drawRoundRect(
                            fill.copy(alpha = 0.35f + 0.65f * percent / 100f),
                            topLeft = Offset(left, size.height - height),
                            size = Size(width, height),
                            cornerRadius = CornerRadius(radius),
                        )
                    }
                }
            }
            // Without Material 3 1.4's gap and its stop dot, as navigation.rs's and the stress
            // screen's.
            LinearProgressIndicator(
                progress = { item.progress / 100f },
                modifier = Modifier.fillMaxWidth(),
                gapSize = 0.dp,
                drawStopIndicator = {},
            )
            Row(
                horizontalArrangement = Arrangement.spacedBy(8.dp),
                verticalAlignment = Alignment.CenterVertically,
            ) {
                Button(onClick = {}) { Text("Open") }
                OutlinedButton(onClick = {}) { Text("Share") }
                Spacer(Modifier.weight(1f))
                Text("${item.progress} %", style = typography.labelLarge)
            }
        }
    }
}

@Composable
private fun Gallery(item: NavigationItem.Gallery) {
    val typography = MaterialTheme.typography
    Column(Modifier.fillMaxWidth().padding(horizontal = 16.dp, vertical = 8.dp)) {
        Row(verticalAlignment = Alignment.CenterVertically) {
            Text(item.album, style = typography.titleMedium, modifier = Modifier.weight(1f))
            Icon(Icons.Filled.ArrowForward, contentDescription = "More")
        }
        Spacer(Modifier.height(8.dp))
        Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
            for (image in item.images) {
                Image(
                    STRESS_BITMAPS[image],
                    contentDescription = null,
                    modifier = Modifier.size(120.dp, 90.dp).clip(RoundedCornerShape(12.dp)),
                    contentScale = ContentScale.Crop,
                )
            }
        }
        Spacer(Modifier.height(4.dp))
        Text(item.caption, style = typography.bodySmall, maxLines = 2, overflow = TextOverflow.Ellipsis)
    }
}

@Composable
private fun Banner(item: NavigationItem.Banner) {
    val shape = RoundedCornerShape(16.dp)
    val typography = MaterialTheme.typography
    // One path, reset for each drawing, as navigation.rs's.
    val path = remember { Path() }
    Box(
        Modifier.fillMaxWidth()
            .padding(horizontal = 16.dp, vertical = 6.dp)
            .height(140.dp)
            .shadow(6.dp, shape, clip = true)
            .background(Brush.linearGradient(item.colors.map { Color(it) })),
    ) {
        Canvas(Modifier.fillMaxSize()) {
            val stroke = 2.dp.toPx()
            val dot = 3.dp.toPx()
            val step = size.width / (SPARKLINE_POINTS - 1)
            path.reset()
            item.points.forEachIndexed { point, percent ->
                val x = point * step
                val y = size.height * (0.15f + 0.5f * (1f - percent / 100f))
                if (point == 0) path.moveTo(x, y) else path.lineTo(x, y)
            }
            drawPath(path, Color.White.copy(alpha = 0.7f), style = Stroke(width = stroke))
            item.points.forEachIndexed { point, percent ->
                if (point % 4 == 0) {
                    val x = point * step
                    val y = size.height * (0.15f + 0.5f * (1f - percent / 100f))
                    drawCircle(Color.White, radius = dot, center = Offset(x, y))
                }
            }
        }
        Column(Modifier.align(Alignment.BottomStart).padding(16.dp)) {
            Text(item.title, style = typography.titleLarge, color = Color.White)
            Text(
                item.line,
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
            Icon(Icons.Filled.Star, contentDescription = null, tint = Color.White)
            Icon(Icons.Filled.Share, contentDescription = null, tint = Color.White)
        }
    }
}
