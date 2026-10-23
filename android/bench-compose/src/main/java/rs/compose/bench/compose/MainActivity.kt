package rs.compose.bench.compose

import android.os.Bundle
import androidx.activity.ComponentActivity
import androidx.activity.compose.setContent
import androidx.activity.enableEdgeToEdge
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
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.verticalScroll
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.Favorite
import androidx.compose.material.icons.filled.Star
import androidx.compose.material3.Card
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Scaffold
import androidx.compose.material3.Text
import androidx.compose.material3.TopAppBar
import androidx.compose.runtime.Composable
import androidx.compose.runtime.Immutable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.key
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp

/**
 * The benchmark's Kotlin Compose app: the screens of examples/feed (compose-rs), widget for
 * widget. `-e screen feed|feed_animated|big|stress|stress_animation` picks one (the stress
 * screens: Stress.kt).
 */
class MainActivity : ComponentActivity() {
    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        enableEdgeToEdge()
        val screen = intent.getStringExtra("screen") ?: "feed"
        setContent {
            MaterialTheme {
                when (screen) {
                    "big" -> BigScreen()
                    "stress" -> StressScroll()
                    "stress_animation" -> StressAnimation()
                    else -> FeedScreen(animated = screen == "feed_animated")
                }
            }
        }
    }
}

const val POSTS = 1_000
const val BIG_POSTS = 200

internal val AUTHORS = listOf(
    "Ada Lovelace", "Alan Turing", "Grace Hopper", "Linus Torvalds", "Barbara Liskov",
    "Donald Knuth", "Margaret Hamilton", "Dennis Ritchie", "Ken Thompson", "Radia Perlman",
    "Edsger Dijkstra", "Frances Allen", "John McCarthy", "Hedy Lamarr", "Tim Berners-Lee",
    "Katherine Johnson",
)

internal val SENTENCES = listOf(
    "Shipped a new release today, with faster startup and fewer allocations.",
    "Reading about incremental computation and how UI frameworks skip work.",
    "Our team switched to declarative UI last year and never looked back.",
    "Profiling showed most of the frame was spent measuring text again.",
    "Coffee, a whiteboard, and a long discussion about state hoisting.",
    "Lazy lists only compose what is visible, plus a few items beyond.",
    "Wrote a small benchmark to compare scrolling on two frameworks.",
    "The best optimization is the work you never have to do at all.",
)

internal val TAGS = listOf("kotlin", "rust", "compose", "android", "performance", "design")

internal val COLORS = longArrayOf(
    0xFF6750A4, 0xFF7D5260, 0xFF006A6A, 0xFF8C4A00, 0xFF3F51B5, 0xFF00796B, 0xFFC2185B, 0xFF5D4037,
)

@Immutable
data class Post(
    val author: String,
    val initials: String,
    val hours: Int,
    val body: String,
    val likes: Int,
    val tag: String,
    val color: Color,
) {
    companion object {
        fun of(index: Int): Post {
            val author = AUTHORS[index % AUTHORS.size]
            return Post(
                author = author,
                initials = author.split(' ').map { it.first() }.joinToString(""),
                hours = (index * 7) % 24 + 1,
                body = SENTENCES[index % SENTENCES.size] + " " + SENTENCES[(index * 3 + 1) % SENTENCES.size],
                likes = (index * 37) % 500,
                tag = TAGS[index % TAGS.size],
                color = Color(COLORS[index % COLORS.size]),
            )
        }
    }
}

@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun FeedScreen(animated: Boolean) {
    Scaffold(topBar = { TopAppBar(title = { Text("Feed") }) }) { padding ->
        LazyColumn(Modifier.fillMaxSize(), contentPadding = padding) {
            // Content types as compose-rs's feed: a post reuses a post that left.
            if (animated) item(contentType = "header") { LiveHeader() }
            items(POSTS, key = { it }, contentType = { "post" }) { index -> PostCard(index) }
        }
    }
}

@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun BigScreen() {
    Scaffold(topBar = { TopAppBar(title = { Text("Big screen") }) }) { padding ->
        Column(Modifier.padding(padding).fillMaxSize().verticalScroll(rememberScrollState())) {
            for (index in 0 until BIG_POSTS) {
                key(index) { PostCard(index) }
            }
        }
    }
}

/** A header animating forever, its values read while drawing (`graphicsLayer { }`). */
@Composable
fun LiveHeader() {
    val transition = rememberInfiniteTransition(label = "Live")
    val pulse = transition.animateFloat(
        initialValue = 0.2f,
        targetValue = 1f,
        animationSpec = infiniteRepeatable(tween(800, easing = LinearEasing), RepeatMode.Reverse),
        label = "pulse",
    )
    val turn = transition.animateFloat(
        initialValue = 0f,
        targetValue = 360f,
        animationSpec = infiniteRepeatable(tween(2000, easing = LinearEasing), RepeatMode.Restart),
        label = "turn",
    )
    val scheme = MaterialTheme.colorScheme
    Card(Modifier.fillMaxWidth().padding(horizontal = 16.dp, vertical = 8.dp)) {
        Row(
            Modifier.padding(16.dp),
            horizontalArrangement = Arrangement.spacedBy(16.dp),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            CircularProgressIndicator(Modifier.size(24.dp))
            Box(
                Modifier.size(12.dp)
                    .graphicsLayer { alpha = pulse.value }
                    .clip(CircleShape)
                    .background(scheme.error),
            )
            Icon(
                Icons.Filled.Star,
                contentDescription = null,
                modifier = Modifier.graphicsLayer { rotationZ = turn.value },
                tint = scheme.primary,
            )
            Text("Live updates", style = MaterialTheme.typography.titleMedium)
        }
    }
}

@Composable
fun PostCard(index: Int) {
    var liked by remember { mutableStateOf(false) }
    val post = remember(index) { Post.of(index) }
    val scheme = MaterialTheme.colorScheme
    val typography = MaterialTheme.typography
    Card(Modifier.fillMaxWidth().padding(horizontal = 16.dp, vertical = 4.dp)) {
        Column(Modifier.padding(16.dp)) {
            Row(verticalAlignment = Alignment.CenterVertically) {
                Box(
                    Modifier.size(40.dp).clip(CircleShape).background(post.color),
                    contentAlignment = Alignment.Center,
                ) {
                    Text(post.initials, color = Color.White, style = typography.titleSmall)
                }
                Spacer(Modifier.width(12.dp))
                Column {
                    Text(post.author, style = typography.titleSmall)
                    Text("${post.hours} h ago", style = typography.bodySmall, color = scheme.onSurfaceVariant)
                }
            }
            Spacer(Modifier.height(8.dp))
            Text(post.body, style = typography.bodyMedium, maxLines = 3, overflow = TextOverflow.Ellipsis)
            Row(verticalAlignment = Alignment.CenterVertically) {
                IconButton(onClick = { liked = !liked }) {
                    Icon(
                        Icons.Filled.Favorite,
                        contentDescription = "Like",
                        tint = if (liked) Color(0xFFE53935) else scheme.outline,
                    )
                }
                Text("${post.likes + if (liked) 1 else 0}", style = typography.labelLarge)
                Spacer(Modifier.weight(1f))
                Text("#${post.tag}", style = typography.labelLarge, color = scheme.primary)
            }
        }
    }
}
