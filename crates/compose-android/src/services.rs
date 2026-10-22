//! Android's platform services (`LOCAL_URI_HANDLER`, `LOCAL_HAPTIC_FEEDBACK`; the clipboard is
//! the bridge's `JniClipboard`), through the view: `ComposeRsView.openUri` starts an
//! `ACTION_VIEW` intent, `performHapticFeedbackType` calls `View.performHapticFeedback`. Both run
//! in the middle of a call into Rust (a click's callback), so the view posts them to the main
//! looper, as it does host events (D226).

use std::rc::Rc;

use compose_ui::{HapticFeedback, HapticFeedbackType, UriHandler, platform::UiHost};
use jni::JNIEnv;
use jni::objects::{GlobalRef, JMethodID, JObject, JValue};
use jni::signature::{Primitive, ReturnType};

use crate::bridge::{check, vm};

/// Sets `ui`'s tree's services to the view's.
pub(crate) fn install(env: &mut JNIEnv, view: &JObject, view_ref: &GlobalRef, ui: &UiHost) {
    let open_uri = method(env, view, "openUri", "(Ljava/lang/String;)V");
    let perform = method(env, view, "performHapticFeedbackType", "(I)V");
    let uri_handler = Rc::new(JniUriHandler { view: view_ref.clone(), method: open_uri });
    let haptic_feedback = Rc::new(JniHapticFeedback { view: view_ref.clone(), method: perform });
    ui.with_tree(|tree| {
        tree.set_uri_handler(uri_handler);
        tree.set_haptic_feedback(haptic_feedback);
    });
}

fn method(env: &mut JNIEnv, view: &JObject, name: &str, signature: &str) -> JMethodID {
    let class = env.get_object_class(view).expect("compose-android: a class");
    let id = env
        .get_method_id(&class, name, signature)
        .unwrap_or_else(|_| panic!("compose-android: ComposeRsView.{name}{signature}"));
    let _ = env.delete_local_ref(class);
    id
}

/// The main thread's environment (Kotlin called in on it).
fn main_env() -> JNIEnv<'static> {
    vm().get_env().expect("compose-android: called on a thread attached to the VM")
}

struct JniUriHandler {
    view: GlobalRef,
    method: JMethodID,
}

impl UriHandler for JniUriHandler {
    fn open_uri(&self, uri: &str) {
        let mut env = main_env();
        let _ = env.with_local_frame(2, |env| -> jni::errors::Result<()> {
            let uri = env.new_string(uri)?;
            // SAFETY: `openUri` is `(Ljava/lang/String;)V`.
            unsafe {
                env.call_method_unchecked(
                    &self.view,
                    self.method,
                    ReturnType::Primitive(Primitive::Void),
                    &[JValue::Object(&uri).as_jni()],
                )?;
            }
            Ok(())
        });
        check(&mut env, "ComposeRsView.openUri");
    }
}

struct JniHapticFeedback {
    view: GlobalRef,
    method: JMethodID,
}

impl HapticFeedback for JniHapticFeedback {
    fn perform_haptic_feedback(&self, feedback_type: HapticFeedbackType) {
        let mut env = main_env();
        // SAFETY: `performHapticFeedbackType` is `(I)V`.
        let _ = unsafe {
            env.call_method_unchecked(
                &self.view,
                self.method,
                ReturnType::Primitive(Primitive::Void),
                &[JValue::Int(haptic_code(feedback_type)).as_jni()],
            )
        };
        check(&mut env, "ComposeRsView.performHapticFeedbackType");
    }
}

/// The code `ComposeRsView.performHapticFeedbackType` maps to a `HapticFeedbackConstants` value:
/// the type's index in Compose's order.
fn haptic_code(feedback_type: HapticFeedbackType) -> i32 {
    feedback_type as i32
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `ComposeRsView.performHapticFeedbackType`'s `when` depends on these.
    #[test]
    fn haptic_codes_match_the_views() {
        use HapticFeedbackType::*;
        let codes: Vec<i32> = [
            LongPress,
            TextHandleMove,
            Confirm,
            ContextClick,
            GestureEnd,
            GestureThresholdActivate,
            KeyboardTap,
            Reject,
            SegmentFrequentTick,
            SegmentTick,
            ToggleOff,
            ToggleOn,
            VirtualKey,
        ]
        .into_iter()
        .map(haptic_code)
        .collect();
        assert_eq!(codes, (0..13).collect::<Vec<_>>());
    }
}
