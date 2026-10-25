package rs.compose.demo

import android.os.Bundle
import android.widget.Toast
import androidx.activity.ComponentActivity
import androidx.activity.compose.setContent
import androidx.activity.enableEdgeToEdge
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.safeDrawingPadding
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Surface
import androidx.compose.material3.Tab
import androidx.compose.material3.TabRow
import androidx.compose.material3.Text
import androidx.compose.runtime.getValue
import androidx.compose.runtime.key
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.viewinterop.AndroidView
import rs.compose.android.ComposeRsRuntime
import rs.compose.android.ComposeRsView

/**
 * The compose-rs examples, each in a `ComposeRsView` inside this Kotlin Compose screen. The views
 * opt in to the performance hint session, as the demo had it before the library's default became
 * none (D581); the other optimizations stay off, as Compose's rendering.
 */
class MainActivity : ComponentActivity() {
    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        ComposeRsRuntime.loadLibrary("compose_rs_demo")
        enableEdgeToEdge()
        val start = intent.getStringExtra("app")
        setContent {
            MaterialTheme {
                Surface(Modifier.fillMaxSize()) {
                    Column(Modifier.safeDrawingPadding()) {
                        val apps = listOf("gallery", "todo", "counter")
                        var selected by rememberSaveable { mutableIntStateOf(apps.indexOf(start).coerceAtLeast(0)) }
                        // Kotlin Compose: the tabs. compose-rs: what's below them.
                        TabRow(selectedTabIndex = selected) {
                            apps.forEachIndexed { index, app ->
                                Tab(selected = index == selected, onClick = { selected = index }, text = { Text(app) })
                            }
                        }
                        key(apps[selected]) {
                            AndroidView(
                                factory = { context ->
                                    ComposeRsView(context, apps[selected]).apply {
                                        // The Rust app's events (a button in the counter).
                                        setOnEventListener { name, payload ->
                                            Toast.makeText(context, "$name: $payload", Toast.LENGTH_SHORT).show()
                                        }
                                    }
                                },
                                modifier = Modifier.fillMaxWidth().weight(1f),
                                // A property the Rust app reads (LOCAL_HOST_PROPS): the tab.
                                update = { view -> view.setProp("tab", "${selected + 1} of ${apps.size}") },
                            )
                        }
                    }
                }
            }
        }
    }
}
