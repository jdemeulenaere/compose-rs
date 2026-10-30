//! `run_compose_ui_test`, as Compose's v2 `runComposeUiTest { … }`: a test body run with a
//! fresh rule, configured by a `ComposeUiTestConfig`, with Compose's failure pipeline (the UI
//! hierarchy and a screenshot captured when the body fails, then the failure handlers).

use std::fmt::Write as _;
use std::panic::{AssertUnwindSafe, catch_unwind, resume_unwind};
use std::rc::Rc;
use std::time::Duration;

use compose_ui::Dp;

use crate::capture::{CapturedImage, has_capture_renderer};
use crate::default_host;
use crate::rule::{ComposeTestRule, DEFAULT_IDLE_TIMEOUT};

/// The test body's rule, as Compose's `ComposeUiTest`: `set_content` once, the finders, the
/// clock, `wait_for_idle`, `run_on_idle`, `wait_until`.
pub type ComposeUiTest = ComposeTestRule;

/// Whether a failure captures an artifact, as Compose's `TestFailurePolicy.CaptureMode`.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum CaptureMode {
    /// The default: a screenshot when a capture renderer is installed, the UI hierarchy
    /// always.
    #[default]
    Unspecified,
    Enabled,
    Disabled,
}

/// A captured artifact's kind, as `FailureArtifact.Type`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum FailureArtifactType {
    Screenshot,
    UiHierarchy,
}

/// What a failure captured, as Compose's `FailureArtifact`: its kind and file name, and its
/// contents (the file is also written under `$COMPOSE_TEST_ARTIFACTS` when that's set).
#[derive(Clone, Debug)]
pub struct FailureArtifact {
    pub kind: FailureArtifactType,
    pub file_name: String,
    /// The UI hierarchy's text.
    pub text: Option<String>,
    /// The window's pixels.
    pub image: Option<CapturedImage>,
}

/// A failure and what it captured, as `FailureContext`.
#[derive(Clone, Debug)]
pub struct FailureContext {
    /// The failure's message (the panic's).
    pub error: String,
    pub artifacts: Vec<FailureArtifact>,
}

/// A handler told of a failure, as `TestFailureHandler`.
pub type TestFailureHandler = Rc<dyn Fn(&FailureContext)>;

/// What a failure captures and who hears of it, as Compose's `TestFailurePolicy`.
#[derive(Clone, Default)]
pub struct TestFailurePolicy {
    pub screenshot_capture_mode: CaptureMode,
    pub ui_hierarchy_capture_mode: CaptureMode,
    pub failure_handlers: Vec<TestFailureHandler>,
}

/// How `run_compose_ui_test_with` sets its rule up, as Compose's `ComposeUiTestConfig`. Of its
/// `effectContext`, only the motion duration scale has a counterpart (D827); `runTestContext` and
/// `inputMode` have none yet (no test dispatchers or input mode).
#[derive(Clone)]
pub struct ComposeUiTestConfig {
    /// The whole test's real-time timeout, as `testTimeout` (one minute): a wait past it, or
    /// the body's end, fails. Each wait for idle is also bounded by a minute of test time.
    pub test_timeout: Duration,
    /// The bounds assertions' tolerance given none (`Dp::UNSPECIFIED`: 0.5 dp), as
    /// `boundsAssertionTolerance`.
    pub bounds_assertion_tolerance: Dp,
    /// The scale animations run at, as a `MotionDurationScale` in `effectContext` (D827).
    pub motion_duration_scale: f32,
    pub failure_policy: TestFailurePolicy,
}

impl Default for ComposeUiTestConfig {
    fn default() -> ComposeUiTestConfig {
        ComposeUiTestConfig {
            test_timeout: DEFAULT_IDLE_TIMEOUT,
            bounds_assertion_tolerance: Dp::UNSPECIFIED,
            motion_duration_scale: 1.0,
            failure_policy: TestFailurePolicy::default(),
        }
    }
}

/// Runs `block` with a fresh rule over the default headless host, as `runComposeUiTest`:
/// it sets the content (`ui.set_content(…)`), acts and asserts. A failure runs the failure
/// pipeline before it carries on.
pub fn run_compose_ui_test<R>(block: impl FnOnce(&mut ComposeUiTest) -> R) -> R {
    run_compose_ui_test_with(ComposeUiTestConfig::default(), block)
}

/// The same, as `runComposeUiTest(config) { … }`.
pub fn run_compose_ui_test_with<R>(
    config: ComposeUiTestConfig,
    block: impl FnOnce(&mut ComposeUiTest) -> R,
) -> R {
    let mut rule = ComposeTestRule::empty(default_host());
    rule.set_test_timeout(config.test_timeout);
    if config.bounds_assertion_tolerance.is_specified() {
        rule.set_bounds_assertion_tolerance(config.bounds_assertion_tolerance);
    }
    if config.motion_duration_scale != 1.0 {
        rule.set_motion_duration_scale(config.motion_duration_scale);
    }
    let outcome = catch_unwind(AssertUnwindSafe(|| {
        let result = block(&mut rule);
        // A body that ran past its time fails, even without waiting at the end.
        rule.check_test_deadline();
        result
    }));
    match outcome {
        Ok(result) => result,
        Err(panic) => {
            let error = panic_message(&*panic);
            rule.clear_test_timeout();
            run_failure_pipeline(&mut rule, &config.failure_policy, error);
            resume_unwind(panic)
        }
    }
}

fn panic_message(panic: &(dyn std::any::Any + Send)) -> String {
    panic
        .downcast_ref::<String>()
        .cloned()
        .or_else(|| panic.downcast_ref::<&str>().map(|text| text.to_string()))
        .unwrap_or_else(|| "a panic".to_string())
}

/// Captures what the policy asks (the hierarchy printed to stderr too), writes the files under
/// `$COMPOSE_TEST_ARTIFACTS` if set, and tells the handlers, each step's own failure noted and
/// skipped, as Compose's `FailurePipelineRunner`.
fn run_failure_pipeline(rule: &mut ComposeTestRule, policy: &TestFailurePolicy, error: String) {
    let time = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |time| time.as_nanos());
    let screenshot = match policy.screenshot_capture_mode {
        CaptureMode::Enabled => true,
        CaptureMode::Disabled => false,
        CaptureMode::Unspecified => has_capture_renderer(),
    };
    let hierarchy = policy.ui_hierarchy_capture_mode != CaptureMode::Disabled;
    let directory = std::env::var_os("COMPOSE_TEST_ARTIFACTS").map(std::path::PathBuf::from);
    if let Some(directory) = &directory
        && let Err(error) = std::fs::create_dir_all(directory)
    {
        eprintln!("compose-test: failed to create {}: {error}", directory.display());
    }
    // The test's thread name (its path, under the default test runner) tells the files of
    // tests that fail at once apart, as Compose's file names carry the test's name.
    let test = artifact_name(std::thread::current().name().unwrap_or("test"));
    let mut artifacts = Vec::new();
    if screenshot {
        let image = catch_unwind(AssertUnwindSafe(|| {
            rule.run_without_implicit_wait(|rule| rule.on_root().capture_to_image())
        }));
        match image {
            Ok(image) => {
                let file_name = format!("{test}_{time}_screenshot.ppm");
                if let Some(directory) = &directory {
                    write_artifact(&directory.join(&file_name), &ppm(&image));
                }
                artifacts.push(FailureArtifact {
                    kind: FailureArtifactType::Screenshot,
                    file_name,
                    text: None,
                    image: Some(image),
                });
            }
            Err(failure) => {
                eprintln!(
                    "compose-test: failed to capture screenshot: {}",
                    panic_message(&*failure)
                )
            }
        }
    }
    if hierarchy {
        let text = catch_unwind(AssertUnwindSafe(|| hierarchy_text(rule)));
        match text {
            Ok(text) => {
                eprintln!("{text}");
                let file_name = format!("{test}_{time}_ui.txt");
                if let Some(directory) = &directory {
                    write_artifact(&directory.join(&file_name), text.as_bytes());
                }
                artifacts.push(FailureArtifact {
                    kind: FailureArtifactType::UiHierarchy,
                    file_name,
                    text: Some(text),
                    image: None,
                });
            }
            Err(failure) => {
                eprintln!("compose-test: failed to dump UI hierarchy: {}", panic_message(&*failure))
            }
        }
    }
    let context = FailureContext { error, artifacts };
    for handler in &policy.failure_handlers {
        if let Err(failure) = catch_unwind(AssertUnwindSafe(|| handler(&context))) {
            eprintln!("compose-test: a failure handler panicked: {}", panic_message(&*failure));
        }
    }
}

/// A thread name as a file name: anything but letters, digits, `-` and `.` becomes `_`.
fn artifact_name(name: &str) -> String {
    name.chars()
        .map(|c| if c.is_ascii_alphanumeric() || c == '-' || c == '.' { c } else { '_' })
        .collect()
}

/// Writes an artifact file, saying so on stderr when it can't.
fn write_artifact(path: &std::path::Path, bytes: &[u8]) {
    if let Err(error) = std::fs::write(path, bytes) {
        eprintln!("compose-test: failed to write {}: {error}", path.display());
    }
}

/// The merged and unmerged semantics trees, as the hierarchy artifact.
fn hierarchy_text(rule: &mut ComposeTestRule) -> String {
    let mut text = String::new();
    let _ = writeln!(text, "====================================================");
    let _ = writeln!(text, "--- Compose Hierarchy ---");
    let _ = writeln!(text, "====================================================");
    let merged = rule.run_without_implicit_wait(|rule| rule.on_root().print_to_string(usize::MAX));
    let unmerged =
        rule.run_without_implicit_wait(|rule| rule.on_root_unmerged().print_to_string(usize::MAX));
    let _ = writeln!(text, "{merged}\n\n{unmerged}");
    text
}

/// An image as a binary PPM (no dependency to encode a PNG).
fn ppm(image: &CapturedImage) -> Vec<u8> {
    let mut bytes = format!("P6\n{} {}\n255\n", image.width(), image.height()).into_bytes();
    for color in image.pixels() {
        bytes.extend([color.red(), color.green(), color.blue()]);
    }
    bytes
}
