package rs.compose.android

import android.view.View
import androidx.lifecycle.Lifecycle
import androidx.lifecycle.LifecycleEventObserver
import androidx.lifecycle.findViewTreeLifecycleOwner

/**
 * A view's lifecycle through its view tree's `LifecycleOwner`. AndroidX is a compile-only
 * dependency of the library: this class is loaded only once [LifecycleSource] found it in the app.
 */
internal object AndroidXLifecycle {
    fun of(view: View, onState: (Int) -> Unit): LifecycleSource? {
        val lifecycle: Lifecycle = view.findViewTreeLifecycleOwner()?.lifecycle ?: return null
        val observer = LifecycleEventObserver { _, _ -> onState(lifecycle.currentState.ordinal) }
        lifecycle.addObserver(observer)
        return object : LifecycleSource {
            override val state: Int
                get() = lifecycle.currentState.ordinal

            override fun remove() = lifecycle.removeObserver(observer)
        }
    }
}
