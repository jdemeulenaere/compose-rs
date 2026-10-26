package rs.compose.bench.rs

import android.app.Activity
import android.graphics.Color
import android.os.Build
import android.os.Bundle
import android.view.View
import android.view.WindowInsetsController
import android.widget.FrameLayout
import rs.compose.android.ComposeRsOptimizations
import rs.compose.android.ComposeRsRuntime
import rs.compose.android.ComposeRsView

/**
 * The benchmark's compose-rs app: a screen of examples/feed in a [ComposeRsView] filling the
 * window, edge to edge as the Compose app (the screens keep their content clear of the system
 * bars as Compose's Material defaults do, D647).
 * `-e screen feed|feed_animated|big|stress|stress_animation|grid|resize|navigation` picks one;
 * `-e optimizations none|recommended|all|default|<names, comma-separated>` compose-rs's own
 * optimizations ([ComposeRsOptimizations]; without it, an app's default: the recommended set,
 * D610). `none` does the work the Compose app does, what measure.py's parity runs pass (D568);
 * measure.py always passes the extra.
 */
class MainActivity : Activity() {
    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        ComposeRsRuntime.loadLibrary("compose_rs_feed")
        val screen = intent.getStringExtra("screen") ?: "feed"
        val optimizations = intent.getStringExtra("optimizations")
            ?.let { ComposeRsOptimizations.parse(it) }
            ?: ComposeRsOptimizations.DEFAULT
        // Not `fitsSystemWindows`: the view draws under the bars and gets their insets.
        val frame = FrameLayout(this)
        frame.addView(ComposeRsView(this, screen, optimizations))
        setContentView(frame)
        enableEdgeToEdge()
    }

    /**
     * The window drawn under transparent system bars, as androidx.activity's `enableEdgeToEdge()`
     * (the Compose app's, with its default styles) on API 29+, without AndroidX: Android 15+
     * enforces it at this target SDK, not the older versions this app runs on (D647). The bars'
     * icons are dark, on the light screens (D645).
     */
    @Suppress("DEPRECATION")
    private fun enableEdgeToEdge() {
        window.statusBarColor = Color.TRANSPARENT
        window.navigationBarColor = Color.TRANSPARENT
        // A scrim behind three-button navigation, none behind the status bar, as its defaults.
        window.isStatusBarContrastEnforced = false
        window.isNavigationBarContrastEnforced = true
        if (Build.VERSION.SDK_INT >= 30) {
            window.setDecorFitsSystemWindows(false)
            val light = WindowInsetsController.APPEARANCE_LIGHT_STATUS_BARS or
                WindowInsetsController.APPEARANCE_LIGHT_NAVIGATION_BARS
            window.insetsController?.setSystemBarsAppearance(light, light)
        } else {
            // What `WindowCompat.setDecorFitsSystemWindows(window, false)` sets before API 30.
            window.decorView.systemUiVisibility = View.SYSTEM_UI_FLAG_LAYOUT_STABLE or
                View.SYSTEM_UI_FLAG_LAYOUT_HIDE_NAVIGATION or
                View.SYSTEM_UI_FLAG_LAYOUT_FULLSCREEN or
                View.SYSTEM_UI_FLAG_LIGHT_STATUS_BAR or
                View.SYSTEM_UI_FLAG_LIGHT_NAVIGATION_BAR
        }
    }
}
