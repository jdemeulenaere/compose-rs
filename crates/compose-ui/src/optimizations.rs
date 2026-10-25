//! compose-rs's own optimizations: what it does beyond Jetpack Compose's rendering, each a flag
//! a host sets (`UiHost::set_optimizations`; `ComposeRsView.optimizations` on Android,
//! `WindowOptions::optimizations` on desktop), so the benchmarks against Compose can measure the
//! same rendering (none of them: `Optimizations::NONE`, parity) and each optimization apart
//! (D520). The default is `recommended`, the flags with a measured gain (D601), on the user's
//! decision (D610; it was parity, D581): a host not told otherwise uses them. Apps can pick
//! `none`, `all`, or the ones they name.
//!
//! Adding one: a constant of [`Optimization`] with the next bit, a row in
//! [`Optimization::ALL`], and the same bit in Kotlin's `ComposeRsOptimizations`
//! (`android/compose-rs`, which a test checks against this table).

use std::fmt;

/// One of compose-rs's optimizations (a bit of [`Optimizations`]).
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct Optimization {
    bit: u8,
    /// Its name in lists (`Optimizations::parse`), as Kotlin's.
    pub name: &'static str,
    /// On unless a host says otherwise.
    pub default: bool,
    /// It changes what's drawn or how: turning it on or off draws everything again (and the
    /// Android serializer writes every record again).
    pub draws: bool,
    /// What it does, and what Compose does instead.
    pub summary: &'static str,
}

impl Optimization {
    /// A graphics layer whose alpha is below 1 under `CompositingStrategy::Auto`, holding one
    /// drawing with a paint and no nested layer, fades that paint instead of compositing its
    /// content offscreen (Android: `setHasOverlappingRendering(false)`; an alpha layer inside a
    /// recording around one such drawing is folded into its paint), as Compose's
    /// `CompositingStrategy.ModulateAlpha` would. The pixels are the same (one drawing can't
    /// overlap itself). Compose's `Auto` draws such a layer offscreen (D153, D157, D521).
    pub const SINGLE_DRAWING_FADE: Optimization = Optimization {
        bit: 0,
        name: "single_drawing_fade",
        default: true,
        draws: true,
        summary: "fade a layer holding one drawing by its paint, not offscreen",
    };

    /// Android: while a view shows (API 33+), the RenderThread and the main thread are in a
    /// performance hint session of compose-rs's own, reporting both threads' work each frame
    /// (`ComposeRsView.isPerformanceHintEnabled`, which it is): it gives compose-rs's threads
    /// the boost HWUI's own session gives Compose's after its longer main-thread frames (D390,
    /// D440, D500). Compose has no such session, so parity runs leave it off; on by default, in
    /// the recommended set (D610; it was on for apps, D390, D568, then off, D581).
    pub const PERFORMANCE_HINT: Optimization = Optimization {
        bit: 1,
        name: "performance_hint",
        default: true,
        draws: false,
        summary: "report the frames' work to a hint session of compose-rs's own (Android)",
    };

    /// A text keeps its paragraph when new constraints change its width bounds but not the
    /// width Compose's `finalMaxWidth` would lay it out at (the text's own width, kept in the
    /// constraints), so a text that fits keeps its layout while its container's width changes.
    /// Compose lays such a text out again (`newLayoutWillBeDifferent`) (D551). On by default, in
    /// the recommended set (D610).
    pub const REUSE_PARAGRAPH_AT_SAME_LAYOUT_WIDTH: Optimization = Optimization {
        bit: 2,
        name: "reuse_paragraph_at_same_layout_width",
        default: true,
        // Read while measuring: what's laid out from then on follows it.
        draws: false,
        summary: "keep a text's paragraph when the width it's laid out at stays the same",
    };

    /// Lazy lists prefetch two items ahead in the scroll direction (a grid: two lines), not
    /// Compose's one (`LazyListPrefetchStrategy`: `last.index + 1`), so a fast fling's frames
    /// find the item after the next composed too (D156, D560).
    pub const PREFETCH_TWO_AHEAD: Optimization = Optimization {
        bit: 3,
        name: "prefetch_two_ahead",
        default: false,
        draws: false,
        summary: "prefetch two items ahead of the scroll, not one",
    };

    /// A prefetch step (composing an item, measuring it) starts only if the time left exceeds,
    /// by 0.5 ms, the larger of its running average and its recent maximum, scaled by how much
    /// slower than estimated this idle time's steps ran (D341, D381); a step that doesn't fit
    /// gives the request up until the next scroll, lowering the estimate; and on Android the
    /// idle time runs to the vsync after the last callback's start, on its frame time's phase
    /// (D340). Compose starts a step whenever the time left exceeds its running average (0 for
    /// an urgent request), keeps a request that doesn't fit for the next frame, and runs to the
    /// last frame's time plus an interval (D561).
    pub const PREFETCH_BUDGET_RECENT_MAXIMUM: Optimization = Optimization {
        bit: 4,
        name: "prefetch_budget_recent_maximum",
        default: false,
        draws: false,
        summary: "budget prefetch steps by their recent maximum with a margin, on the vsync's phase",
    };

    /// Lazy lists also prefetch at rest, after a first layout or a jump, towards the last scroll
    /// direction (the end before any), so a fling's first frames find the next items composed
    /// (D470). Compose prefetches only on scrolls (`onScroll`) (D562).
    pub const PREFETCH_AT_REST: Optimization = Optimization {
        bit: 5,
        name: "prefetch_at_rest",
        default: false,
        draws: false,
        summary: "prefetch the next items after a layout at rest, not only on scrolls",
    };

    /// Lazy lists keep up to two composed items behind the scroll, so a fling reversing finds
    /// the first items it wants composed, and items beyond are disposed between frames, by the
    /// next prefetch's composition (D375); a frame lists the kept items' groups, so Android
    /// keeps their recordings (D380). Compose deactivates an item in the measurement it leaves,
    /// and keeps none behind (D563).
    pub const KEEP_ITEMS_BEHIND: Optimization = Optimization {
        bit: 6,
        name: "keep_items_behind",
        default: false,
        draws: false,
        summary: "keep two items behind the scroll composed, and their recordings",
    };

    /// Android: the items a prefetch measured are drawn ahead in the same idle time (their
    /// groups drawn and recorded, D350), and the paragraphs it laid out recorded into their
    /// render nodes (D322), so the frame showing the item records nothing. Compose's prefetch
    /// composes and measures; nothing records until a frame places and draws the item (D564).
    pub const DRAW_PREFETCHED_AHEAD: Optimization = Optimization {
        bit: 7,
        name: "draw_prefetched_ahead",
        default: true,
        draws: false,
        summary: "draw and record prefetched items between frames (Android)",
    };

    /// Lazy grids compose and place two lines beyond each side of their viewport (D456).
    /// Compose's grids compose the lines in view (`beyondBoundsItemCount` 0) and prefetch one
    /// line ahead (D565).
    pub const GRID_LINES_BEYOND: Optimization = Optimization {
        bit: 8,
        name: "grid_lines_beyond",
        default: false,
        draws: false,
        summary: "compose and place two grid lines beyond each side of the viewport",
    };

    /// Android: a frame dropping a path drawn as a path draws a cleared 1 x 1 compositing layer
    /// first, so HWUI flushes its layers before the frame and Skia's GPU cache reuses the dropped
    /// paths' mask textures (D510). Compose draws no such layer (D566).
    pub const FLUSH_LAYERS_BEFORE_DROPPED_PATHS: Optimization = Optimization {
        bit: 9,
        name: "flush_layers_before_dropped_paths",
        default: true,
        draws: false,
        summary: "flush HWUI's layers before a frame dropping a stroked path (Android)",
    };

    /// Android: each paragraph is recorded once into a render node of its own, which the
    /// recordings drawing it draw by reference (D153). Compose draws a paragraph's `Layout` into
    /// the recording of the layer it's in, each time that layer records (D567).
    pub const PARAGRAPH_RENDER_NODES: Optimization = Optimization {
        bit: 10,
        name: "paragraph_render_nodes",
        default: false,
        // Every recording drawing a paragraph draws it differently.
        draws: true,
        summary: "record each paragraph once into a render node of its own (Android)",
    };

    /// Every optimization, by bit.
    pub const ALL: &'static [Optimization] = &[
        Optimization::SINGLE_DRAWING_FADE,
        Optimization::PERFORMANCE_HINT,
        Optimization::REUSE_PARAGRAPH_AT_SAME_LAYOUT_WIDTH,
        Optimization::PREFETCH_TWO_AHEAD,
        Optimization::PREFETCH_BUDGET_RECENT_MAXIMUM,
        Optimization::PREFETCH_AT_REST,
        Optimization::KEEP_ITEMS_BEHIND,
        Optimization::DRAW_PREFETCHED_AHEAD,
        Optimization::GRID_LINES_BEYOND,
        Optimization::FLUSH_LAYERS_BEFORE_DROPPED_PATHS,
        Optimization::PARAGRAPH_RENDER_NODES,
    ];

    /// The recommended ones ([`Optimizations::recommended`]): those with a gain measured on the
    /// device, each alone against parity (docs/14, "Backport candidates") and together against
    /// `all` (docs/12; D601). `flush_layers_before_dropped_paths` is `single_drawing_fade`'s
    /// companion: the fade alone grows HWUI's GPU caches by 31 % on a screen dropping stroked
    /// paths every frame, and the flush layer brings them back. They're the ones on by default
    /// (D610).
    pub const RECOMMENDED: &'static [Optimization] = &[
        Optimization::SINGLE_DRAWING_FADE,
        Optimization::PERFORMANCE_HINT,
        Optimization::REUSE_PARAGRAPH_AT_SAME_LAYOUT_WIDTH,
        Optimization::DRAW_PREFETCHED_AHEAD,
        Optimization::FLUSH_LAYERS_BEFORE_DROPPED_PATHS,
    ];

    /// Its bit in [`Optimizations::bits`].
    pub const fn mask(self) -> u64 {
        1 << self.bit
    }

    /// The optimization named `name`, if any.
    pub fn named(name: &str) -> Option<Optimization> {
        Optimization::ALL.iter().copied().find(|optimization| optimization.name == name)
    }
}

/// Which of compose-rs's optimizations a host uses: a set of [`Optimization`]s, as bits (what
/// crosses to Kotlin).
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct Optimizations(u64);

impl Optimizations {
    /// None: the rendering Compose does.
    pub const NONE: Optimizations = Optimizations(0);

    /// The ones on by default ([`Optimization::default`]): the recommended set (D610; parity,
    /// none, before: D581).
    pub fn defaults() -> Optimizations {
        Optimization::ALL
            .iter()
            .filter(|optimization| optimization.default)
            .fold(Optimizations::NONE, |set, optimization| set.with(*optimization, true))
    }

    /// The recommended ones ([`Optimization::RECOMMENDED`]): those with a measured gain, without
    /// those that measured none or a cost (D601). The default ([`Optimizations::defaults`],
    /// D610).
    pub fn recommended() -> Optimizations {
        Optimization::RECOMMENDED
            .iter()
            .fold(Optimizations::NONE, |set, optimization| set.with(*optimization, true))
    }

    /// Every one.
    pub fn all() -> Optimizations {
        Optimization::ALL
            .iter()
            .fold(Optimizations::NONE, |set, optimization| set.with(*optimization, true))
    }

    pub const fn contains(self, optimization: Optimization) -> bool {
        self.0 & optimization.mask() != 0
    }

    /// This set with `optimization` on or off.
    pub const fn with(self, optimization: Optimization, on: bool) -> Optimizations {
        if on {
            Optimizations(self.0 | optimization.mask())
        } else {
            Optimizations(self.0 & !optimization.mask())
        }
    }

    pub const fn bits(self) -> u64 {
        self.0
    }

    /// Whether going from `self` to `other` changes one that draws ([`Optimization::draws`]).
    pub fn draws_differently(self, other: Optimizations) -> bool {
        Optimization::ALL.iter().any(|optimization| {
            optimization.draws && self.contains(*optimization) != other.contains(*optimization)
        })
    }

    /// The set of `bits` (unknown bits dropped).
    pub fn from_bits(bits: u64) -> Optimizations {
        Optimizations(bits & Optimizations::all().0)
    }

    /// Parses `none`, `all`, `recommended`, `default`, or a comma-separated list of names (each
    /// may start with `-` to take it out of what's before it: `all,-performance_hint`,
    /// `recommended,-performance_hint`); a set's name replaces what's before it, a flag's adds to
    /// it (`recommended,keep_items_behind`); an empty text is the defaults. An unknown name is an
    /// error naming it.
    pub fn parse(text: &str) -> Result<Optimizations, String> {
        if text.trim().is_empty() {
            return Ok(Optimizations::defaults());
        }
        let mut set = Optimizations::NONE;
        for word in text.split(',').map(str::trim).filter(|word| !word.is_empty()) {
            let (on, name) = match word.strip_prefix('-') {
                Some(name) => (false, name),
                None => (true, word),
            };
            set = match (name, on) {
                ("none", true) => Optimizations::NONE,
                ("all", true) => Optimizations::all(),
                ("recommended", true) => Optimizations::recommended(),
                ("default", true) => Optimizations::defaults(),
                _ => match Optimization::named(name) {
                    Some(optimization) => set.with(optimization, on),
                    None => return Err(format!("no optimization named {name:?}")),
                },
            };
        }
        Ok(set)
    }
}

impl Default for Optimizations {
    fn default() -> Optimizations {
        Optimizations::defaults()
    }
}

impl fmt::Debug for Optimizations {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let names: Vec<&str> = Optimization::ALL
            .iter()
            .filter(|optimization| self.contains(**optimization))
            .map(|optimization| optimization.name)
            .collect();
        write!(
            formatter,
            "Optimizations({})",
            if names.is_empty() { "none".to_string() } else { names.join(",") }
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bits_are_distinct_and_in_order() {
        for (index, optimization) in Optimization::ALL.iter().enumerate() {
            assert_eq!(optimization.bit as usize, index, "{}", optimization.name);
        }
    }

    #[test]
    fn lists_parse() {
        let fade = Optimization::SINGLE_DRAWING_FADE;
        let hint = Optimization::PERFORMANCE_HINT;
        assert_eq!(Optimizations::parse("none"), Ok(Optimizations::NONE));
        // Nothing named: the defaults (an empty `COMPOSE_OPTIMIZATIONS`, `-e optimizations ""`).
        assert_eq!(Optimizations::parse(" "), Ok(Optimizations::defaults()));
        assert!(Optimizations::NONE.draws_differently(Optimizations::NONE.with(fade, true)));
        assert!(!Optimizations::NONE.draws_differently(Optimizations::NONE.with(hint, true)));
        assert_eq!(Optimizations::parse("all"), Ok(Optimizations::all()));
        assert_eq!(Optimizations::parse("default"), Ok(Optimizations::defaults()));
        // The library's default is the recommended set, the hint session included (D610).
        assert_eq!(Optimizations::defaults(), Optimizations::recommended());
        assert_eq!(Optimizations::default(), Optimizations::recommended());
        assert_eq!(Optimizations::parse(""), Ok(Optimizations::recommended()));
        assert_eq!(Optimizations::parse("default"), Ok(Optimizations::recommended()));
        for optimization in Optimization::ALL {
            assert_eq!(
                optimization.default,
                Optimization::RECOMMENDED.contains(optimization),
                "{}",
                optimization.name
            );
        }
        assert!(Optimizations::defaults().contains(hint));
        assert!(Optimizations::defaults().contains(fade));
        // Parity is still none: no flag.
        assert_eq!(Optimizations::NONE.bits(), 0);
        assert_eq!(
            Optimizations::parse("all,-performance_hint"),
            Ok(Optimizations::all().with(hint, false))
        );
        assert_eq!(
            Optimizations::parse("single_drawing_fade"),
            Ok(Optimizations::NONE.with(fade, true))
        );
        assert_eq!(
            Optimizations::parse("single_drawing_fade, performance_hint, -performance_hint"),
            Ok(Optimizations::NONE.with(fade, true))
        );
        assert!(Optimizations::parse("faster").is_err());
        assert_eq!(Optimizations::from_bits(u64::MAX), Optimizations::all());
        assert_eq!(
            format!("{:?}", Optimizations::all()),
            "Optimizations(single_drawing_fade,performance_hint,reuse_paragraph_at_same_layout_width,\
             prefetch_two_ahead,prefetch_budget_recent_maximum,prefetch_at_rest,keep_items_behind,\
             draw_prefetched_ahead,grid_lines_beyond,flush_layers_before_dropped_paths,\
             paragraph_render_nodes)"
        );
    }

    /// `recommended` is the flags with a measured gain (D601): the hint session, drawing
    /// prefetched items ahead, the single-drawing fade with its companion flush layer, and the
    /// paragraph kept at the same layout width; not those that measured none or a cost.
    #[test]
    fn the_recommended_set() {
        let recommended = Optimizations::recommended();
        assert_eq!(
            format!("{recommended:?}"),
            "Optimizations(single_drawing_fade,performance_hint,reuse_paragraph_at_same_layout_width,\
             draw_prefetched_ahead,flush_layers_before_dropped_paths)"
        );
        for optimization in [
            Optimization::PREFETCH_TWO_AHEAD,
            Optimization::PREFETCH_BUDGET_RECENT_MAXIMUM,
            Optimization::PREFETCH_AT_REST,
            Optimization::KEEP_ITEMS_BEHIND,
            Optimization::GRID_LINES_BEYOND,
            Optimization::PARAGRAPH_RENDER_NODES,
        ] {
            assert!(!recommended.contains(optimization), "{}", optimization.name);
        }
        // The fade comes with its companion (the fade alone grows the GPU caches by 31 %).
        assert!(recommended.contains(Optimization::SINGLE_DRAWING_FADE));
        assert!(recommended.contains(Optimization::FLUSH_LAYERS_BEFORE_DROPPED_PATHS));
        // It draws differently from parity (the fade), and is the default (D610).
        assert!(Optimizations::NONE.draws_differently(recommended));
        assert_eq!(Optimizations::defaults(), recommended);
        let mut table = Optimization::RECOMMENDED.to_vec();
        table.dedup();
        assert_eq!(table.len(), 5);
        assert!(table.iter().all(|optimization| Optimization::ALL.contains(optimization)));
    }

    #[test]
    fn recommended_parses_and_combines() {
        let hint = Optimization::PERFORMANCE_HINT;
        let behind = Optimization::KEEP_ITEMS_BEHIND;
        let recommended = Optimizations::recommended();
        assert_eq!(Optimizations::parse("recommended"), Ok(recommended));
        assert_eq!(Optimizations::parse(" recommended "), Ok(recommended));
        assert_eq!(
            Optimizations::parse("recommended,-performance_hint"),
            Ok(recommended.with(hint, false))
        );
        assert_eq!(
            Optimizations::parse("recommended,keep_items_behind"),
            Ok(recommended.with(behind, true))
        );
        // A set's name replaces what's before it.
        assert_eq!(Optimizations::parse("keep_items_behind,recommended"), Ok(recommended));
        assert_eq!(Optimizations::parse("all,recommended"), Ok(recommended));
        assert_eq!(Optimizations::parse("recommended,all"), Ok(Optimizations::all()));
        assert_eq!(Optimizations::parse("recommended,none"), Ok(Optimizations::NONE));
        // Only a set's name, not its removal: `-recommended` is unknown, as `-all` is.
        assert!(Optimizations::parse("all,-recommended").is_err());
        assert!(Optimizations::parse("-all").is_err());
        assert_eq!(Optimizations::from_bits(recommended.bits()), recommended);
    }

    /// A host not told otherwise uses the recommended set (D610).
    #[test]
    fn a_host_defaults_to_the_recommended_set() {
        let mut host = crate::host::UiHost::headless(10, 10);
        assert_eq!(host.optimizations(), Optimizations::recommended());
        host.set_optimizations(Optimizations::NONE);
        assert_eq!(host.optimizations(), Optimizations::NONE);
    }
}
