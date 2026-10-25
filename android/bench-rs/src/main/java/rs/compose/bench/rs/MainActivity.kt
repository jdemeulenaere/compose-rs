package rs.compose.bench.rs

import android.app.Activity
import android.os.Bundle
import android.view.WindowInsetsController
import android.widget.FrameLayout
import rs.compose.android.ComposeRsOptimizations
import rs.compose.android.ComposeRsRuntime
import rs.compose.android.ComposeRsView

/**
 * The benchmark's compose-rs app: a screen of examples/feed in a [ComposeRsView], below the
 * status bar. `-e screen feed|feed_animated|big|stress|stress_animation|grid|resize` picks one;
 * `-e optimizations none|all|default|<names, comma-separated>` compose-rs's own optimizations
 * ([ComposeRsOptimizations]; without it, an app's default: none, D581). `none` does the work
 * the Compose app does, what measure.py's parity runs pass (D568).
 */
class MainActivity : Activity() {
    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        ComposeRsRuntime.loadLibrary("compose_rs_feed")
        val screen = intent.getStringExtra("screen") ?: "feed"
        val optimizations = intent.getStringExtra("optimizations")
            ?.let { ComposeRsOptimizations.parse(it) }
            ?: ComposeRsOptimizations.DEFAULT
        val frame = FrameLayout(this)
        frame.fitsSystemWindows = true
        frame.addView(ComposeRsView(this, screen, optimizations))
        setContentView(frame)
        // Dark status bar icons on the light app, as the Compose app's edge-to-edge bars.
        window.insetsController?.setSystemBarsAppearance(
            WindowInsetsController.APPEARANCE_LIGHT_STATUS_BARS or WindowInsetsController.APPEARANCE_LIGHT_NAVIGATION_BARS,
            WindowInsetsController.APPEARANCE_LIGHT_STATUS_BARS or WindowInsetsController.APPEARANCE_LIGHT_NAVIGATION_BARS,
        )
    }
}
