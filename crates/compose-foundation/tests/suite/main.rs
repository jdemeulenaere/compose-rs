//! The compose-foundation integration tests in one binary, each file a module, so
//! they build and link once (docs/08-fast-builds.md).

extern crate compose_runtime as compose;

mod draw_modifiers;
mod effects;
mod foundation;
mod grid;
mod hover;
mod image;
mod lazy;
mod pointer_input;
mod review_e;
mod review_f;
mod review_f2;
mod review_f3;
mod review_h;
mod review_image;
mod review_lazy;
mod review_lazy2;
mod review_overlay;
mod review_overlay2;
mod review_round10;
mod review_round3;
mod review_round5;
mod review_round6;
mod review_round7;
mod review_round9;
mod review_rtl;
mod review_semantics;
mod review_text_draw;
mod rtl;
mod scroll;
mod scrollbar;
mod semantics_actions;
mod shadow;
mod window;
