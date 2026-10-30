package rs.compose.android

import android.view.View
import androidx.lifecycle.ViewModel
import androidx.lifecycle.ViewModelProvider
import androidx.lifecycle.findViewTreeViewModelStoreOwner

/**
 * Rust's view models live with a holder among the view tree's `ViewModelStoreOwner`'s: kept across
 * a configuration change, cleared when the owner's cleared for good. AndroidX is a compile-only
 * dependency of the library: this class is loaded only once [HostViewModels] found it in the app.
 */
internal object AndroidXViewModels {
    fun tokenOf(view: View): Long {
        val owner = view.findViewTreeViewModelStoreOwner() ?: return 0
        val provider = ViewModelProvider(owner.viewModelStore, Factory)
        return provider[KEY, NativeViewModels::class.java].token
    }

    private object Factory : ViewModelProvider.Factory {
        override fun <T : ViewModel> create(modelClass: Class<T>): T =
            modelClass.cast(NativeViewModels())!!
    }

    private const val KEY = "rs.compose.android.NativeViewModels"
}

/** One store of Rust's, for all the views under its owner, as their composables share one. */
internal class NativeViewModels : ViewModel() {
    val token: Long = ComposeRsNative.newViewModelStore()

    override fun onCleared() = ComposeRsNative.clearViewModelStore(token)
}
