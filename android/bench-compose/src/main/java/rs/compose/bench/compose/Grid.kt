package rs.compose.bench.compose

import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.aspectRatio
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.lazy.grid.GridCells
import androidx.compose.foundation.lazy.grid.LazyVerticalGrid
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.Favorite
import androidx.compose.material.icons.filled.Star
import androidx.compose.material.icons.filled.ThumbUp
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Scaffold
import androidx.compose.material3.Text
import androidx.compose.material3.TopAppBar
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.unit.dp

/*
 * The grid screen of examples/feed (compose-rs, `grid.rs`), widget for widget: a lazy grid of
 * tiles, for ad hoc fling traces of lazy grids. Not one of measure.py's scenarios.
 *
 * `grid`: a top app bar over a `LazyVerticalGrid` of 3,000 keyed tiles, `Adaptive(100.dp)`, 8 dp
 * apart. A tile is a square in one of the palette's colors, rounded 12 dp, with an icon and its
 * label; the icon is one of three, its content type.
 */

const val GRID_TILES = 3_000

/** The kind of tile `index` is, which picks its icon: also its `contentType`. */
private fun gridKind(index: Int) = index % 3

@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun GridScreen() {
    Scaffold(topBar = { TopAppBar(title = { Text("Grid") }) }) { padding ->
        LazyVerticalGrid(
            columns = GridCells.Adaptive(100.dp),
            modifier = Modifier.padding(padding).fillMaxSize(),
            contentPadding = PaddingValues(8.dp),
            verticalArrangement = Arrangement.spacedBy(8.dp),
            horizontalArrangement = Arrangement.spacedBy(8.dp),
        ) {
            items(GRID_TILES, key = { it }, contentType = { gridKind(it) }) { GridTile(it) }
        }
    }
}

@Composable
private fun GridTile(index: Int) {
    val icon = when (gridKind(index)) {
        0 -> Icons.Filled.Star
        1 -> Icons.Filled.Favorite
        else -> Icons.Filled.ThumbUp
    }
    Box(
        Modifier
            .fillMaxWidth()
            .aspectRatio(1f)
            .clip(RoundedCornerShape(12.dp))
            .background(Color(COLORS[index % 8])),
        contentAlignment = Alignment.Center,
    ) {
        Column(horizontalAlignment = Alignment.CenterHorizontally) {
            Icon(icon, contentDescription = null, tint = Color.White)
            Text("Tile ${index + 1}", style = MaterialTheme.typography.labelMedium, color = Color.White)
        }
    }
}
