package rs.compose.android

import android.view.View

/** The Rust side's view model store a view's host takes. */
internal object HostViewModels {
    /**
     * The token of the store kept by [view]'s view tree's `ViewModelStoreOwner`, as Compose's
     * `LocalViewModelStoreOwner`; 0 without one: the host has a store of its own, lost with it.
     */
    fun tokenOf(view: View): Long {
        if (androidXPresent) {
            try {
                return AndroidXViewModels.tokenOf(view)
            } catch (_: NoClassDefFoundError) {
                // The app has no AndroidX view models (or R8 removed them).
                androidXPresent = false
            }
        }
        return 0
    }

    private var androidXPresent = true
}
