package rs.compose.android

import android.app.Activity
import androidx.activity.OnBackPressedCallback
import androidx.activity.OnBackPressedDispatcherOwner

/**
 * Back through an AndroidX activity's `OnBackPressedDispatcher`. AndroidX is a compile-only
 * dependency of the library: this class is loaded only once `BackRegistration` found it in the
 * app.
 */
internal object AndroidXBack {
    fun register(activity: Activity, onBack: () -> Unit): BackRegistration? {
        val owner = activity as? OnBackPressedDispatcherOwner ?: return null
        val dispatcher = owner.onBackPressedDispatcher
        val callback = object : OnBackPressedCallback(false) {
            override fun handleOnBackPressed() = onBack()
        }
        dispatcher.addCallback(callback)
        return object : BackRegistration {
            override val handlesKeys: Boolean
                get() = false

            override fun setEnabled(enabled: Boolean) {
                callback.isEnabled = enabled
            }

            override fun remove() = callback.remove()

            override fun passOn() {
                // The content's handler went in the meantime: the dispatcher's next callback.
                callback.isEnabled = false
                dispatcher.onBackPressed()
            }
        }
    }
}
