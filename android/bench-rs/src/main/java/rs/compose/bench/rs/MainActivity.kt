package rs.compose.bench.rs

import android.app.Activity
import android.content.res.Configuration
import android.graphics.Color
import android.os.Build
import android.os.Bundle
import android.view.View
import android.view.WindowInsetsController
import android.view.WindowManager
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
     * The window drawn under transparent system bars, as androidx.activity 1.13's
     * `enableEdgeToEdge()` with its default styles (`SystemBarStyle.auto`, the Compose app's)
     * does on API 29+, without AndroidX: Android 15+ enforces edge to edge at this target SDK,
     * not the older versions this app runs on (D647). The bars' icons follow the configuration's
     * night mode, as `auto`'s (light bars, dark icons, unless it's night), whatever the screens'
     * colors (D645); the system's contrast scrim behind three-button navigation follows them.
     */
    @Suppress("DEPRECATION")
    private fun enableEdgeToEdge() {
        val night = (resources.configuration.uiMode and Configuration.UI_MODE_NIGHT_MASK) ==
            Configuration.UI_MODE_NIGHT_YES
        window.statusBarColor = Color.TRANSPARENT
        window.navigationBarColor = Color.TRANSPARENT
        // No scrim behind the status bar; the system's behind three-button navigation.
        window.isStatusBarContrastEnforced = false
        window.isNavigationBarContrastEnforced = true
        // Into the cutout, as its `adjustLayoutInDisplayCutoutMode`.
        window.attributes = window.attributes.apply {
            layoutInDisplayCutoutMode = if (Build.VERSION.SDK_INT >= 30) {
                WindowManager.LayoutParams.LAYOUT_IN_DISPLAY_CUTOUT_MODE_ALWAYS
            } else {
                WindowManager.LayoutParams.LAYOUT_IN_DISPLAY_CUTOUT_MODE_SHORT_EDGES
            }
        }
        if (Build.VERSION.SDK_INT >= 30) {
            window.setDecorFitsSystemWindows(false)
            val light = WindowInsetsController.APPEARANCE_LIGHT_STATUS_BARS or
                WindowInsetsController.APPEARANCE_LIGHT_NAVIGATION_BARS
            window.insetsController?.setSystemBarsAppearance(if (night) 0 else light, light)
        } else {
            // What `WindowCompat.setDecorFitsSystemWindows(window, false)` sets before API 30.
            val layout = View.SYSTEM_UI_FLAG_LAYOUT_STABLE or View.SYSTEM_UI_FLAG_LAYOUT_HIDE_NAVIGATION or
                View.SYSTEM_UI_FLAG_LAYOUT_FULLSCREEN
            val light = View.SYSTEM_UI_FLAG_LIGHT_STATUS_BAR or View.SYSTEM_UI_FLAG_LIGHT_NAVIGATION_BAR
            window.decorView.systemUiVisibility = if (night) layout else layout or light
        }
    }
}
