package rs.compose.bench.rs

import android.app.Activity
import android.os.Bundle
import android.view.WindowInsetsController
import android.widget.FrameLayout
import rs.compose.android.ComposeRsOptimizations
import rs.compose.android.ComposeRsRuntime
import rs.compose.android.ComposeRsView

/**
 * The benchmark's compose-rs app: a screen of examples/feed in a [ComposeRsView] filling the
 * window, edge to edge as the Compose app (the screens keep their content clear of the system
 * bars as Compose's Material defaults do, D647).
 * `-e screen feed|feed_animated|big|stress|stress_animation|grid|resize|navigation` picks one; `-e optimizations none|recommended|all|default|<names, comma-separated>`
 * compose-rs's own optimizations ([ComposeRsOptimizations]; without it, an app's default: the
 * recommended set, D610). `none` does the work the Compose app does, what measure.py's parity
 * runs pass (D568); measure.py always passes the extra.
 */
class MainActivity : Activity() {
    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        ComposeRsRuntime.loadLibrary("compose_rs_feed")
        val screen = intent.getStringExtra("screen") ?: "feed"
        val optimizations = intent.getStringExtra("optimizations")
            ?.let { ComposeRsOptimizations.parse(it) }
            ?: ComposeRsOptimizations.DEFAULT
        // Not `fitsSystemWindows`: the view draws under the bars, as the Compose app's
        // `enableEdgeToEdge()` (which Android 15+ enforces at this targetSdk), and gets the
        // bars' insets.
        val frame = FrameLayout(this)
        frame.addView(ComposeRsView(this, screen, optimizations))
        setContentView(frame)
        // Dark status bar icons on the light app, as the Compose app's edge-to-edge bars.
        window.insetsController?.setSystemBarsAppearance(
            WindowInsetsController.APPEARANCE_LIGHT_STATUS_BARS or WindowInsetsController.APPEARANCE_LIGHT_NAVIGATION_BARS,
            WindowInsetsController.APPEARANCE_LIGHT_STATUS_BARS or WindowInsetsController.APPEARANCE_LIGHT_NAVIGATION_BARS,
        )
    }
}
