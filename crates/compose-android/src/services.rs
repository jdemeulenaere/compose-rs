//! Android's platform services (`LOCAL_URI_HANDLER`, `LOCAL_HAPTIC_FEEDBACK`; the clipboard is
//! the bridge's `JniClipboard`), through the view: `ComposeRsView.openUri` starts an
//! `ACTION_VIEW` intent, `performHapticFeedbackType` calls `View.performHapticFeedback`. Both run
//! in the middle of a call into Rust (a click's callback), so the view posts them to the main
//! looper, as it does host events (D226). Links are checked first, in Rust
//! (`PlatformUriHandler`: allowed schemes only, D283).

use std::rc::Rc;

use compose_ui::{HapticFeedback, HapticFeedbackType, PlatformUriHandler, platform::UiHost};
use jni::ids::JMethodID;
use jni::objects::JObject;
use jni::refs::Global;
use jni::signature::{Primitive, ReturnType};
use jni::{Env, JValue, jni_sig, jni_str};

use crate::bridge::{check, method, with_main_env};

/// Sets `ui`'s tree's services to the view's.
pub(crate) fn install(
    env: &mut Env,
    view: &JObject,
    view_ref: &Rc<Global<JObject<'static>>>,
    ui: &UiHost,
) {
    let open_uri = method(env, view, jni_str!("openUri"), jni_sig!("(Ljava/lang/String;)V"));
    let perform = method(env, view, jni_str!("performHapticFeedbackType"), jni_sig!("(I)V"));
    let opener = JniUriOpener { view: view_ref.clone(), method: open_uri };
    let uri_handler = PlatformUriHandler::new(move |uri| opener.open(uri));
    let haptic_feedback = Rc::new(JniHapticFeedback { view: view_ref.clone(), method: perform });
    ui.with_tree(|tree| {
        tree.set_platform_uri_handler(uri_handler);
        tree.set_haptic_feedback(haptic_feedback);
    });
}

/// Hands a checked link to `ComposeRsView.openUri`.
struct JniUriOpener {
    view: Rc<Global<JObject<'static>>>,
    method: JMethodID,
}

impl JniUriOpener {
    fn open(&self, uri: &str) {
        with_main_env(|env| {
            let _ = env.with_local_frame(2, |env| -> jni::errors::Result<()> {
                let uri = env.new_string(uri)?;
                // SAFETY: `openUri` is `(Ljava/lang/String;)V`.
                unsafe {
                    env.call_method_unchecked(
                        &**self.view,
                        self.method,
                        ReturnType::Primitive(Primitive::Void),
                        &[JValue::Object(&uri).as_jni()],
                    )?;
                }
                Ok(())
            });
            check(env, "ComposeRsView.openUri");
        });
    }
}

struct JniHapticFeedback {
    view: Rc<Global<JObject<'static>>>,
    method: JMethodID,
}

impl HapticFeedback for JniHapticFeedback {
    fn perform_haptic_feedback(&self, feedback_type: HapticFeedbackType) {
        with_main_env(|env| {
            // SAFETY: `performHapticFeedbackType` is `(I)V`.
            let _ = unsafe {
                env.call_method_unchecked(
                    &**self.view,
                    self.method,
                    ReturnType::Primitive(Primitive::Void),
                    &[JValue::Int(haptic_code(feedback_type)).as_jni()],
                )
            };
            check(env, "ComposeRsView.performHapticFeedbackType");
        });
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
