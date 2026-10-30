# Design decisions

The decision log is the record of why compose-rs is built the way it is: each decision, the reason for it, the alternatives rejected (often with the numbers behind the choice), and who decided. It is history. How things work now is in the design documents ([01](01-feasibility.md) to [17](17-navigation-latency.md)) and the [developer guide](guide/README.md); a reader never needs the log to understand current behaviour, and each row links to the section that holds its detail.

The log is split by area into the files below, one table each, rows in number order. A split into current rows and an archive was the other option. It reads worse here: of 366 rows, 71 are revised by later ones and 11 superseded, and most chains stay in one area (a lazy-list rule, its review fixes, then its flag: 107 of the 126 links from a row to the rows revising it stay within one file), so an area file keeps a decision next to the rows that refine it, and the status column says which still hold. When the log was split, each row was shortened and the detail that describes current behaviour moved into the doc sections the rows link to; the log as one table, with its rows' full wording, is `docs/decisions.md` at commit 4146ff5.

## Conventions

- **Numbers** (D1, D2, …) are stable and never reused: other docs, code comments and commit messages refer to them, and every number below still has its row. Gaps (D165 to D169, …) are numbers never used: blocks were reserved per piece of work.
- **A row** says what was decided and why, then the main alternatives rejected and why not, then the revert cost. The detail of how it works now is in the doc section the row links to.
- **Who decided:** "(user)" or "on the user's decision" for the user's decisions, "(agent)" for decisions an agent made on the user's behalf, "the coordinator's" or "the orchestrator's decision" for those the agent coordinating the work made; "after a review" marks a fix a review asked for.
- **Status:** *current* (the decision holds); *revised by Dxxx* (a later row changed part of it: read both, the later one wins where they differ); *superseded by Dxxx* (replaced as a whole: history only); *reverted* (taken out before or after merging: history only, its row saying why).
- **Revert cost:** *low* (a local edit), *medium* (one subsystem), *high* (touches most of the code).
- **New decisions** take the next number after the highest (D600 at this split, so D601 next; one is being added on another branch), go into the area file that fits them, in number order, with a line in the index below, and change the status of the rows they revise or supersede.

## Files

| File | Covers | Rows |
|---|---|---|
| [runtime](decisions/runtime.md) | `#[composable]`, groups and keys, skipping, lambdas, the call macro, the composer, recomposition, `remember`, effects, state handles, pausable composition. | 108 |
| [ui-and-layout](decisions/ui-and-layout.md) | Modifiers and nodes, layout and intrinsics, drawing in the tree, input, focus, semantics, animation, Material 3, navigation, platform values. | 139 |
| [text](decisions/text.md) | Text measurement and layout on each platform, fonts, text fields and editing. | 42 |
| [lazy-and-prefetch](decisions/lazy-and-prefetch.md) | Lazy lists and grids, item reuse, the policy of composing only what is in view, prefetch. | 64 |
| [android-drawing](decisions/android-drawing.md) | The display list and its replayer, render nodes and repaint boundaries, paths, shadows, layers. | 27 |
| [android-host-and-performance](decisions/android-host-and-performance.md) | The view and activity glue, the frame loop, the hint session, jank, startup, memory, and the optimization flags. | 25 |
| [desktop](decisions/desktop.md) | The desktop host: window, Skia rendering, input and events. | 6 |
| [benchmarks](decisions/benchmarks.md) | The benchmark apps, protocols and rounds, and how results are read. | 34 |
| [api-and-tooling](decisions/api-and-tooling.md) | Names, crates and modules, semver, the edit-build loop, testing and documentation, the toolchain's and dependencies' versions. | 38 |

## Index by number

| # | File | Status |
|---|---|---|
| D1 | [runtime](decisions/runtime.md#d1) | current |
| D2 | [runtime](decisions/runtime.md#d2) | current |
| D3 | [runtime](decisions/runtime.md#d3) | revised by [D47](decisions/runtime.md#d47) |
| D4 | [runtime](decisions/runtime.md#d4) | current |
| D5 | [runtime](decisions/runtime.md#d5) | current |
| D6 | [runtime](decisions/runtime.md#d6) | current |
| D7 | [runtime](decisions/runtime.md#d7) | current |
| D8 | [runtime](decisions/runtime.md#d8) | current |
| D9 | [runtime](decisions/runtime.md#d9) | current |
| D10 | [runtime](decisions/runtime.md#d10) | current |
| D11 | [runtime](decisions/runtime.md#d11) | current |
| D12 | [api-and-tooling](decisions/api-and-tooling.md#d12) | current |
| D13 | [api-and-tooling](decisions/api-and-tooling.md#d13) | current |
| D14 | [runtime](decisions/runtime.md#d14) | current |
| D15 | [runtime](decisions/runtime.md#d15) | revised by [D703](decisions/runtime.md#d703) |
| D16 | [runtime](decisions/runtime.md#d16) | revised by [D702](decisions/runtime.md#d702) |
| D17 | [runtime](decisions/runtime.md#d17) | current |
| D18 | [ui-and-layout](decisions/ui-and-layout.md#d18) | revised by [D1151](decisions/ui-and-layout.md#d1151) |
| D19 | [ui-and-layout](decisions/ui-and-layout.md#d19) | current |
| D20 | [ui-and-layout](decisions/ui-and-layout.md#d20) | current |
| D21 | [text](decisions/text.md#d21) | current |
| D22 | [desktop](decisions/desktop.md#d22) | current |
| D23 | [desktop](decisions/desktop.md#d23) | current |
| D24 | [api-and-tooling](decisions/api-and-tooling.md#d24) | revised by [D304](decisions/api-and-tooling.md#d304) |
| D25 | [android-host-and-performance](decisions/android-host-and-performance.md#d25) | current |
| D26 | [android-drawing](decisions/android-drawing.md#d26) | current |
| D27 | [android-host-and-performance](decisions/android-host-and-performance.md#d27) | current |
| D28 | [runtime](decisions/runtime.md#d28) | current |
| D29 | [runtime](decisions/runtime.md#d29) | current |
| D30 | [runtime](decisions/runtime.md#d30) | current |
| D31 | [runtime](decisions/runtime.md#d31) | revised by [D712](decisions/runtime.md#d712) |
| D32 | [runtime](decisions/runtime.md#d32) | current |
| D33 | [runtime](decisions/runtime.md#d33) | current |
| D34 | [runtime](decisions/runtime.md#d34) | current |
| D35 | [runtime](decisions/runtime.md#d35) | current |
| D36 | [runtime](decisions/runtime.md#d36) | current |
| D37 | [runtime](decisions/runtime.md#d37) | current |
| D38 | [runtime](decisions/runtime.md#d38) | current |
| D39 | [runtime](decisions/runtime.md#d39) | current |
| D40 | [runtime](decisions/runtime.md#d40) | superseded by [D703](decisions/runtime.md#d703) |
| D41 | [runtime](decisions/runtime.md#d41) | current |
| D42 | [runtime](decisions/runtime.md#d42) | current |
| D43 | [runtime](decisions/runtime.md#d43) | current |
| D44 | [runtime](decisions/runtime.md#d44) | current |
| D45 | [runtime](decisions/runtime.md#d45) | current |
| D46 | [api-and-tooling](decisions/api-and-tooling.md#d46) | current |
| D47 | [runtime](decisions/runtime.md#d47) | current |
| D48 | [runtime](decisions/runtime.md#d48) | current |
| D49 | [runtime](decisions/runtime.md#d49) | current |
| D50 | [runtime](decisions/runtime.md#d50) | current |
| D51 | [runtime](decisions/runtime.md#d51) | current |
| D52 | [runtime](decisions/runtime.md#d52) | current |
| D53 | [runtime](decisions/runtime.md#d53) | current |
| D54 | [runtime](decisions/runtime.md#d54) | current |
| D55 | [runtime](decisions/runtime.md#d55) | current |
| D56 | [runtime](decisions/runtime.md#d56) | current |
| D57 | [runtime](decisions/runtime.md#d57) | current |
| D58 | [runtime](decisions/runtime.md#d58) | current |
| D59 | [runtime](decisions/runtime.md#d59) | current |
| D60 | [runtime](decisions/runtime.md#d60) | current |
| D61 | [api-and-tooling](decisions/api-and-tooling.md#d61) | current |
| D62 | [runtime](decisions/runtime.md#d62) | current |
| D63 | [runtime](decisions/runtime.md#d63) | current |
| D64 | [runtime](decisions/runtime.md#d64) | current |
| D65 | [runtime](decisions/runtime.md#d65) | current |
| D66 | [runtime](decisions/runtime.md#d66) | current |
| D67 | [ui-and-layout](decisions/ui-and-layout.md#d67) | current |
| D68 | [ui-and-layout](decisions/ui-and-layout.md#d68) | current |
| D69 | [ui-and-layout](decisions/ui-and-layout.md#d69) | current |
| D70 | [runtime](decisions/runtime.md#d70) | current |
| D71 | [ui-and-layout](decisions/ui-and-layout.md#d71) | current |
| D72 | [android-drawing](decisions/android-drawing.md#d72) | superseded by [D155](decisions/android-drawing.md#d155) |
| D73 | [ui-and-layout](decisions/ui-and-layout.md#d73) | current |
| D74 | [ui-and-layout](decisions/ui-and-layout.md#d74) | current |
| D75 | [ui-and-layout](decisions/ui-and-layout.md#d75) | current |
| D76 | [ui-and-layout](decisions/ui-and-layout.md#d76) | current |
| D77 | [ui-and-layout](decisions/ui-and-layout.md#d77) | current |
| D78 | [api-and-tooling](decisions/api-and-tooling.md#d78) | current |
| D79 | [text](decisions/text.md#d79) | current |
| D80 | [runtime](decisions/runtime.md#d80) | revised by [D89](decisions/runtime.md#d89) |
| D81 | [runtime](decisions/runtime.md#d81) | revised by [D97](decisions/runtime.md#d97), [D310](decisions/runtime.md#d310) |
| D82 | [runtime](decisions/runtime.md#d82) | current |
| D83 | [runtime](decisions/runtime.md#d83) | superseded by [D115](decisions/runtime.md#d115) |
| D84 | [runtime](decisions/runtime.md#d84) | current |
| D85 | [ui-and-layout](decisions/ui-and-layout.md#d85) | current |
| D86 | [ui-and-layout](decisions/ui-and-layout.md#d86) | current |
| D87 | [runtime](decisions/runtime.md#d87) | current |
| D88 | [runtime](decisions/runtime.md#d88) | current |
| D89 | [runtime](decisions/runtime.md#d89) | current |
| D90 | [ui-and-layout](decisions/ui-and-layout.md#d90) | current |
| D91 | [api-and-tooling](decisions/api-and-tooling.md#d91) | current |
| D92 | [runtime](decisions/runtime.md#d92) | current |
| D93 | [desktop](decisions/desktop.md#d93) | current |
| D94 | [text](decisions/text.md#d94) | current |
| D95 | [ui-and-layout](decisions/ui-and-layout.md#d95) | current |
| D96 | [text](decisions/text.md#d96) | current |
| D97 | [runtime](decisions/runtime.md#d97) | revised by [D713](decisions/runtime.md#d713) |
| D98 | [runtime](decisions/runtime.md#d98) | current |
| D99 | [runtime](decisions/runtime.md#d99) | current |
| D100 | [ui-and-layout](decisions/ui-and-layout.md#d100) | revised by [D125](decisions/lazy-and-prefetch.md#d125) |
| D101 | [ui-and-layout](decisions/ui-and-layout.md#d101) | current |
| D102 | [text](decisions/text.md#d102) | current |
| D103 | [text](decisions/text.md#d103) | current |
| D104 | [text](decisions/text.md#d104) | current |
| D105 | [runtime](decisions/runtime.md#d105) | current |
| D106 | [runtime](decisions/runtime.md#d106) | current |
| D107 | [runtime](decisions/runtime.md#d107) | current |
| D108 | [runtime](decisions/runtime.md#d108) | current |
| D109 | [runtime](decisions/runtime.md#d109) | superseded by [D122](decisions/runtime.md#d122) |
| D110 | [runtime](decisions/runtime.md#d110) | current |
| D111 | [runtime](decisions/runtime.md#d111) | current |
| D112 | [api-and-tooling](decisions/api-and-tooling.md#d112) | revised by [D900](decisions/api-and-tooling.md#d900) |
| D113 | [desktop](decisions/desktop.md#d113) | current |
| D114 | [runtime](decisions/runtime.md#d114) | current |
| D115 | [runtime](decisions/runtime.md#d115) | current |
| D116 | [runtime](decisions/runtime.md#d116) | current |
| D117 | [text](decisions/text.md#d117) | current |
| D118 | [text](decisions/text.md#d118) | current |
| D119 | [text](decisions/text.md#d119) | current |
| D120 | [ui-and-layout](decisions/ui-and-layout.md#d120) | current |
| D121 | [ui-and-layout](decisions/ui-and-layout.md#d121) | current |
| D122 | [runtime](decisions/runtime.md#d122) | revised by [D711](decisions/runtime.md#d711) |
| D123 | [runtime](decisions/runtime.md#d123) | current |
| D124 | [text](decisions/text.md#d124) | current |
| D125 | [lazy-and-prefetch](decisions/lazy-and-prefetch.md#d125) | revised by [D156](decisions/lazy-and-prefetch.md#d156), [D470](decisions/lazy-and-prefetch.md#d470), [D487](decisions/lazy-and-prefetch.md#d487), [D575](decisions/lazy-and-prefetch.md#d575) |
| D126 | [ui-and-layout](decisions/ui-and-layout.md#d126) | current |
| D127 | [ui-and-layout](decisions/ui-and-layout.md#d127) | superseded by [D138](decisions/ui-and-layout.md#d138) |
| D128 | [ui-and-layout](decisions/ui-and-layout.md#d128) | revised by [D187](decisions/ui-and-layout.md#d187) |
| D129 | [ui-and-layout](decisions/ui-and-layout.md#d129) | superseded by [D188](decisions/ui-and-layout.md#d188) |
| D130 | [ui-and-layout](decisions/ui-and-layout.md#d130) | current |
| D131 | [ui-and-layout](decisions/ui-and-layout.md#d131) | current |
| D132 | [lazy-and-prefetch](decisions/lazy-and-prefetch.md#d132) | current |
| D133 | [ui-and-layout](decisions/ui-and-layout.md#d133) | current |
| D134 | [desktop](decisions/desktop.md#d134) | current |
| D135 | [ui-and-layout](decisions/ui-and-layout.md#d135) | current |
| D136 | [lazy-and-prefetch](decisions/lazy-and-prefetch.md#d136) | revised by [D470](decisions/lazy-and-prefetch.md#d470), [D477](decisions/lazy-and-prefetch.md#d477) |
| D137 | [ui-and-layout](decisions/ui-and-layout.md#d137) | superseded by [D1200](decisions/ui-and-layout.md#d1200) |
| D138 | [ui-and-layout](decisions/ui-and-layout.md#d138) | revised by [D184](decisions/ui-and-layout.md#d184), [D524](decisions/ui-and-layout.md#d524) |
| D139 | [ui-and-layout](decisions/ui-and-layout.md#d139) | current |
| D140 | [text](decisions/text.md#d140) | current |
| D141 | [api-and-tooling](decisions/api-and-tooling.md#d141) | current |
| D142 | [ui-and-layout](decisions/ui-and-layout.md#d142) | revised by [D450](decisions/lazy-and-prefetch.md#d450), [D701](decisions/ui-and-layout.md#d701) |
| D143 | [ui-and-layout](decisions/ui-and-layout.md#d143) | current |
| D144 | [lazy-and-prefetch](decisions/lazy-and-prefetch.md#d144) | superseded by [D450](decisions/lazy-and-prefetch.md#d450) |
| D145 | [runtime](decisions/runtime.md#d145) | current |
| D146 | [ui-and-layout](decisions/ui-and-layout.md#d146) | current |
| D147 | [text](decisions/text.md#d147) | current |
| D148 | [ui-and-layout](decisions/ui-and-layout.md#d148) | current |
| D149 | [desktop](decisions/desktop.md#d149) | current |
| D150 | [ui-and-layout](decisions/ui-and-layout.md#d150) | superseded by [D816](decisions/ui-and-layout.md#d816) |
| D151 | [text](decisions/text.md#d151) | current |
| D152 | [android-drawing](decisions/android-drawing.md#d152) | revised by [D524](decisions/ui-and-layout.md#d524) |
| D153 | [android-drawing](decisions/android-drawing.md#d153) | revised by [D521](decisions/android-drawing.md#d521), [D567](decisions/android-drawing.md#d567) |
| D154 | [android-drawing](decisions/android-drawing.md#d154) | revised by [D157](decisions/android-drawing.md#d157) |
| D155 | [android-drawing](decisions/android-drawing.md#d155) | revised by [D270](decisions/android-drawing.md#d270), [D323](decisions/android-drawing.md#d323) |
| D156 | [lazy-and-prefetch](decisions/lazy-and-prefetch.md#d156) | revised by [D340](decisions/android-host-and-performance.md#d340), [D375](decisions/lazy-and-prefetch.md#d375), [D470](decisions/lazy-and-prefetch.md#d470), [D560](decisions/lazy-and-prefetch.md#d560), [D575](decisions/lazy-and-prefetch.md#d575) |
| D157 | [android-drawing](decisions/android-drawing.md#d157) | revised by [D270](decisions/android-drawing.md#d270), [D323](decisions/android-drawing.md#d323), [D521](decisions/android-drawing.md#d521), [D525](decisions/android-drawing.md#d525) |
| D158 | [runtime](decisions/runtime.md#d158) | current |
| D159 | [runtime](decisions/runtime.md#d159) | current |
| D160 | [runtime](decisions/runtime.md#d160) | current |
| D161 | [api-and-tooling](decisions/api-and-tooling.md#d161) | current |
| D162 | [runtime](decisions/runtime.md#d162) | current |
| D163 | [runtime](decisions/runtime.md#d163) | current |
| D164 | [api-and-tooling](decisions/api-and-tooling.md#d164) | current |
| D170 | [lazy-and-prefetch](decisions/lazy-and-prefetch.md#d170) | current |
| D171 | [lazy-and-prefetch](decisions/lazy-and-prefetch.md#d171) | current |
| D172 | [ui-and-layout](decisions/ui-and-layout.md#d172) | current |
| D173 | [ui-and-layout](decisions/ui-and-layout.md#d173) | current |
| D174 | [lazy-and-prefetch](decisions/lazy-and-prefetch.md#d174) | current |
| D175 | [ui-and-layout](decisions/ui-and-layout.md#d175) | revised by [D210](decisions/android-drawing.md#d210), [D270](decisions/android-drawing.md#d270) |
| D176 | [ui-and-layout](decisions/ui-and-layout.md#d176) | revised by [D530](decisions/ui-and-layout.md#d530) |
| D177 | [ui-and-layout](decisions/ui-and-layout.md#d177) | current |
| D180 | [ui-and-layout](decisions/ui-and-layout.md#d180) | current |
| D181 | [ui-and-layout](decisions/ui-and-layout.md#d181) | current |
| D182 | [ui-and-layout](decisions/ui-and-layout.md#d182) | superseded by [D188](decisions/ui-and-layout.md#d188) |
| D183 | [ui-and-layout](decisions/ui-and-layout.md#d183) | revised by [D222](decisions/ui-and-layout.md#d222) |
| D184 | [ui-and-layout](decisions/ui-and-layout.md#d184) | revised by [D526](decisions/ui-and-layout.md#d526) |
| D185 | [ui-and-layout](decisions/ui-and-layout.md#d185) | current |
| D186 | [ui-and-layout](decisions/ui-and-layout.md#d186) | current |
| D187 | [ui-and-layout](decisions/ui-and-layout.md#d187) | current |
| D188 | [ui-and-layout](decisions/ui-and-layout.md#d188) | current |
| D189 | [ui-and-layout](decisions/ui-and-layout.md#d189) | current |
| D190 | [android-drawing](decisions/android-drawing.md#d190) | revised by [D411](decisions/android-drawing.md#d411) |
| D195 | [android-drawing](decisions/android-drawing.md#d195) | current |
| D200 | [runtime](decisions/runtime.md#d200) | current |
| D210 | [android-drawing](decisions/android-drawing.md#d210) | current |
| D220 | [ui-and-layout](decisions/ui-and-layout.md#d220) | current |
| D221 | [android-host-and-performance](decisions/android-host-and-performance.md#d221) | current |
| D222 | [ui-and-layout](decisions/ui-and-layout.md#d222) | current |
| D223 | [runtime](decisions/runtime.md#d223) | current |
| D224 | [ui-and-layout](decisions/ui-and-layout.md#d224) | current |
| D225 | [ui-and-layout](decisions/ui-and-layout.md#d225) | current |
| D226 | [ui-and-layout](decisions/ui-and-layout.md#d226) | current |
| D230 | [ui-and-layout](decisions/ui-and-layout.md#d230) | current |
| D231 | [runtime](decisions/runtime.md#d231) | current |
| D232 | [runtime](decisions/runtime.md#d232) | current |
| D233 | [text](decisions/text.md#d233) | revised by [D1105](decisions/text.md#d1105) |
| D234 | [text](decisions/text.md#d234) | current |
| D235 | [text](decisions/text.md#d235) | current |
| D240 | [runtime](decisions/runtime.md#d240) | current |
| D241 | [ui-and-layout](decisions/ui-and-layout.md#d241) | current |
| D250 | [text](decisions/text.md#d250) | current |
| D251 | [android-drawing](decisions/android-drawing.md#d251) | revised by [D522](decisions/android-drawing.md#d522) |
| D260 | [api-and-tooling](decisions/api-and-tooling.md#d260) | current |
| D261 | [api-and-tooling](decisions/api-and-tooling.md#d261) | current |
| D262 | [api-and-tooling](decisions/api-and-tooling.md#d262) | current |
| D263 | [api-and-tooling](decisions/api-and-tooling.md#d263) | current |
| D264 | [api-and-tooling](decisions/api-and-tooling.md#d264) | current |
| D265 | [api-and-tooling](decisions/api-and-tooling.md#d265) | current |
| D266 | [api-and-tooling](decisions/api-and-tooling.md#d266) | current |
| D270 | [android-drawing](decisions/android-drawing.md#d270) | revised by [D323](decisions/android-drawing.md#d323) |
| D280 | [api-and-tooling](decisions/api-and-tooling.md#d280) | current |
| D281 | [ui-and-layout](decisions/ui-and-layout.md#d281) | revised by [D283](decisions/ui-and-layout.md#d283) |
| D282 | [ui-and-layout](decisions/ui-and-layout.md#d282) | current |
| D283 | [ui-and-layout](decisions/ui-and-layout.md#d283) | current |
| D290 | [runtime](decisions/runtime.md#d290) | revised by [D430](decisions/lazy-and-prefetch.md#d430) |
| D291 | [ui-and-layout](decisions/ui-and-layout.md#d291) | current |
| D292 | [api-and-tooling](decisions/api-and-tooling.md#d292) | current |
| D293 | [ui-and-layout](decisions/ui-and-layout.md#d293) | current |
| D300 | [android-host-and-performance](decisions/android-host-and-performance.md#d300) | current |
| D301 | [android-host-and-performance](decisions/android-host-and-performance.md#d301) | revised by [D623](decisions/api-and-tooling.md#d623) |
| D302 | [benchmarks](decisions/benchmarks.md#d302) | revised by [D362](decisions/benchmarks.md#d362) |
| D303 | [api-and-tooling](decisions/api-and-tooling.md#d303) | current |
| D304 | [api-and-tooling](decisions/api-and-tooling.md#d304) | current |
| D310 | [runtime](decisions/runtime.md#d310) | current |
| D320 | [ui-and-layout](decisions/ui-and-layout.md#d320) | current |
| D321 | [android-host-and-performance](decisions/android-host-and-performance.md#d321) | current |
| D322 | [android-host-and-performance](decisions/android-host-and-performance.md#d322) | revised by [D341](decisions/lazy-and-prefetch.md#d341), [D350](decisions/lazy-and-prefetch.md#d350), [D564](decisions/lazy-and-prefetch.md#d564) |
| D323 | [android-drawing](decisions/android-drawing.md#d323) | current |
| D330 | [text](decisions/text.md#d330) | revised by [D550](decisions/text.md#d550), [D571](decisions/text.md#d571) |
| D340 | [android-host-and-performance](decisions/android-host-and-performance.md#d340) | revised by [D561](decisions/lazy-and-prefetch.md#d561) |
| D341 | [lazy-and-prefetch](decisions/lazy-and-prefetch.md#d341) | revised by [D381](decisions/lazy-and-prefetch.md#d381), [D561](decisions/lazy-and-prefetch.md#d561) |
| D342 | [android-host-and-performance](decisions/android-host-and-performance.md#d342) | current |
| D343 | [benchmarks](decisions/benchmarks.md#d343) | current |
| D350 | [lazy-and-prefetch](decisions/lazy-and-prefetch.md#d350) | revised by [D564](decisions/lazy-and-prefetch.md#d564) |
| D360 | [android-host-and-performance](decisions/android-host-and-performance.md#d360) | current |
| D361 | [android-host-and-performance](decisions/android-host-and-performance.md#d361) | current |
| D362 | [benchmarks](decisions/benchmarks.md#d362) | current |
| D370 | [benchmarks](decisions/benchmarks.md#d370) | revised by [D390](decisions/android-host-and-performance.md#d390) |
| D375 | [lazy-and-prefetch](decisions/lazy-and-prefetch.md#d375) | revised by [D563](decisions/lazy-and-prefetch.md#d563), [D575](decisions/lazy-and-prefetch.md#d575) |
| D380 | [android-drawing](decisions/android-drawing.md#d380) | revised by [D563](decisions/lazy-and-prefetch.md#d563) |
| D381 | [lazy-and-prefetch](decisions/lazy-and-prefetch.md#d381) | revised by [D561](decisions/lazy-and-prefetch.md#d561) |
| D390 | [android-host-and-performance](decisions/android-host-and-performance.md#d390) | revised by [D440](decisions/android-host-and-performance.md#d440), [D500](decisions/android-host-and-performance.md#d500), [D581](decisions/android-host-and-performance.md#d581) |
| D400 | [benchmarks](decisions/benchmarks.md#d400) | current |
| D401 | [benchmarks](decisions/benchmarks.md#d401) | current |
| D402 | [benchmarks](decisions/benchmarks.md#d402) | revised by [D521](decisions/android-drawing.md#d521) |
| D410 | [android-drawing](decisions/android-drawing.md#d410) | revised by [D530](decisions/ui-and-layout.md#d530), [D532](decisions/android-drawing.md#d532) |
| D411 | [android-drawing](decisions/android-drawing.md#d411) | current |
| D412 | [ui-and-layout](decisions/ui-and-layout.md#d412) | superseded by [D530](decisions/ui-and-layout.md#d530) |
| D420 | [ui-and-layout](decisions/ui-and-layout.md#d420) | revised by [D822](decisions/ui-and-layout.md#d822) |
| D421 | [runtime](decisions/runtime.md#d421) | current |
| D422 | [ui-and-layout](decisions/ui-and-layout.md#d422) | current |
| D423 | [ui-and-layout](decisions/ui-and-layout.md#d423) | superseded by [D803](decisions/ui-and-layout.md#d803) |
| D424 | [benchmarks](decisions/benchmarks.md#d424) | current |
| D430 | [lazy-and-prefetch](decisions/lazy-and-prefetch.md#d430) | revised by [D460](decisions/lazy-and-prefetch.md#d460) |
| D431 | [lazy-and-prefetch](decisions/lazy-and-prefetch.md#d431) | revised by [D434](decisions/lazy-and-prefetch.md#d434), [D435](decisions/lazy-and-prefetch.md#d435), [D1153](decisions/ui-and-layout.md#d1153) |
| D432 | [lazy-and-prefetch](decisions/lazy-and-prefetch.md#d432) | current |
| D433 | [lazy-and-prefetch](decisions/lazy-and-prefetch.md#d433) | current |
| D434 | [lazy-and-prefetch](decisions/lazy-and-prefetch.md#d434) | revised by [D435](decisions/lazy-and-prefetch.md#d435), [D1155](decisions/ui-and-layout.md#d1155) |
| D435 | [lazy-and-prefetch](decisions/lazy-and-prefetch.md#d435) | revised by [D1156](decisions/ui-and-layout.md#d1156) |
| D440 | [android-host-and-performance](decisions/android-host-and-performance.md#d440) | revised by [D500](decisions/android-host-and-performance.md#d500) |
| D441 | [android-host-and-performance](decisions/android-host-and-performance.md#d441) | current |
| D442 | [android-host-and-performance](decisions/android-host-and-performance.md#d442) | superseded by [D500](decisions/android-host-and-performance.md#d500) |
| D450 | [lazy-and-prefetch](decisions/lazy-and-prefetch.md#d450) | revised by [D456](decisions/lazy-and-prefetch.md#d456), [D560](decisions/lazy-and-prefetch.md#d560) |
| D451 | [lazy-and-prefetch](decisions/lazy-and-prefetch.md#d451) | revised by [D454](decisions/lazy-and-prefetch.md#d454) |
| D452 | [lazy-and-prefetch](decisions/lazy-and-prefetch.md#d452) | current |
| D453 | [lazy-and-prefetch](decisions/lazy-and-prefetch.md#d453) | revised by [D455](decisions/lazy-and-prefetch.md#d455), [D458](decisions/lazy-and-prefetch.md#d458) |
| D454 | [lazy-and-prefetch](decisions/lazy-and-prefetch.md#d454) | current |
| D455 | [lazy-and-prefetch](decisions/lazy-and-prefetch.md#d455) | revised by [D458](decisions/lazy-and-prefetch.md#d458) |
| D456 | [lazy-and-prefetch](decisions/lazy-and-prefetch.md#d456) | revised by [D470](decisions/lazy-and-prefetch.md#d470), [D560](decisions/lazy-and-prefetch.md#d560), [D565](decisions/lazy-and-prefetch.md#d565) |
| D457 | [lazy-and-prefetch](decisions/lazy-and-prefetch.md#d457) | current |
| D458 | [lazy-and-prefetch](decisions/lazy-and-prefetch.md#d458) | current |
| D459 | [lazy-and-prefetch](decisions/lazy-and-prefetch.md#d459) | current |
| D460 | [lazy-and-prefetch](decisions/lazy-and-prefetch.md#d460) | current |
| D461 | [runtime](decisions/runtime.md#d461) | current |
| D470 | [lazy-and-prefetch](decisions/lazy-and-prefetch.md#d470) | revised by [D477](decisions/lazy-and-prefetch.md#d477), [D562](decisions/lazy-and-prefetch.md#d562) |
| D471 | [lazy-and-prefetch](decisions/lazy-and-prefetch.md#d471) | current |
| D472 | [lazy-and-prefetch](decisions/lazy-and-prefetch.md#d472) | revised by [D475](decisions/lazy-and-prefetch.md#d475), [D487](decisions/lazy-and-prefetch.md#d487) |
| D473 | [benchmarks](decisions/benchmarks.md#d473) | current |
| D474 | [lazy-and-prefetch](decisions/lazy-and-prefetch.md#d474) | current |
| D475 | [lazy-and-prefetch](decisions/lazy-and-prefetch.md#d475) | revised by [D481](decisions/lazy-and-prefetch.md#d481), [D487](decisions/lazy-and-prefetch.md#d487) |
| D476 | [lazy-and-prefetch](decisions/lazy-and-prefetch.md#d476) | revised by [D479](decisions/lazy-and-prefetch.md#d479) |
| D477 | [lazy-and-prefetch](decisions/lazy-and-prefetch.md#d477) | revised by [D483](decisions/lazy-and-prefetch.md#d483), [D495](decisions/lazy-and-prefetch.md#d495) |
| D478 | [ui-and-layout](decisions/ui-and-layout.md#d478) | revised by [D484](decisions/ui-and-layout.md#d484), [D491](decisions/ui-and-layout.md#d491) |
| D479 | [lazy-and-prefetch](decisions/lazy-and-prefetch.md#d479) | revised by [D488](decisions/lazy-and-prefetch.md#d488), [D489](decisions/lazy-and-prefetch.md#d489), [D490](decisions/lazy-and-prefetch.md#d490), [D493](decisions/lazy-and-prefetch.md#d493), [D494](decisions/lazy-and-prefetch.md#d494) |
| D480 | [lazy-and-prefetch](decisions/lazy-and-prefetch.md#d480) | current |
| D481 | [lazy-and-prefetch](decisions/lazy-and-prefetch.md#d481) | superseded by [D487](decisions/lazy-and-prefetch.md#d487) |
| D482 | [lazy-and-prefetch](decisions/lazy-and-prefetch.md#d482) | current |
| D483 | [lazy-and-prefetch](decisions/lazy-and-prefetch.md#d483) | revised by [D486](decisions/lazy-and-prefetch.md#d486) |
| D484 | [ui-and-layout](decisions/ui-and-layout.md#d484) | revised by [D491](decisions/ui-and-layout.md#d491) |
| D485 | [lazy-and-prefetch](decisions/lazy-and-prefetch.md#d485) | current |
| D486 | [lazy-and-prefetch](decisions/lazy-and-prefetch.md#d486) | current |
| D487 | [lazy-and-prefetch](decisions/lazy-and-prefetch.md#d487) | revised by [D492](decisions/lazy-and-prefetch.md#d492) |
| D488 | [lazy-and-prefetch](decisions/lazy-and-prefetch.md#d488) | current |
| D489 | [lazy-and-prefetch](decisions/lazy-and-prefetch.md#d489) | revised by [D493](decisions/lazy-and-prefetch.md#d493) |
| D490 | [lazy-and-prefetch](decisions/lazy-and-prefetch.md#d490) | current |
| D491 | [ui-and-layout](decisions/ui-and-layout.md#d491) | current |
| D492 | [lazy-and-prefetch](decisions/lazy-and-prefetch.md#d492) | current |
| D493 | [lazy-and-prefetch](decisions/lazy-and-prefetch.md#d493) | current |
| D494 | [lazy-and-prefetch](decisions/lazy-and-prefetch.md#d494) | current |
| D495 | [lazy-and-prefetch](decisions/lazy-and-prefetch.md#d495) | current |
| D496 | [ui-and-layout](decisions/ui-and-layout.md#d496) | current |
| D500 | [android-host-and-performance](decisions/android-host-and-performance.md#d500) | revised by [D581](decisions/android-host-and-performance.md#d581) |
| D501 | [android-host-and-performance](decisions/android-host-and-performance.md#d501) | current |
| D502 | [android-host-and-performance](decisions/android-host-and-performance.md#d502) | current |
| D510 | [android-drawing](decisions/android-drawing.md#d510) | revised by [D566](decisions/android-drawing.md#d566) |
| D511 | [android-host-and-performance](decisions/android-host-and-performance.md#d511) | current |
| D512 | [ui-and-layout](decisions/ui-and-layout.md#d512) | superseded by [D530](decisions/ui-and-layout.md#d530), [D534](decisions/android-drawing.md#d534) |
| D520 | [android-host-and-performance](decisions/android-host-and-performance.md#d520) | revised by [D581](decisions/android-host-and-performance.md#d581), [D601](decisions/android-host-and-performance.md#d601) |
| D521 | [android-drawing](decisions/android-drawing.md#d521) | revised by [D648](decisions/android-drawing.md#d648) |
| D522 | [android-drawing](decisions/android-drawing.md#d522) | current |
| D523 | [ui-and-layout](decisions/ui-and-layout.md#d523) | current |
| D524 | [ui-and-layout](decisions/ui-and-layout.md#d524) | current |
| D525 | [android-drawing](decisions/android-drawing.md#d525) | current |
| D526 | [ui-and-layout](decisions/ui-and-layout.md#d526) | current |
| D527 | [benchmarks](decisions/benchmarks.md#d527) | current |
| D528 | [ui-and-layout](decisions/ui-and-layout.md#d528) | current |
| D529 | [android-drawing](decisions/android-drawing.md#d529) | current |
| D530 | [ui-and-layout](decisions/ui-and-layout.md#d530) | current |
| D531 | [ui-and-layout](decisions/ui-and-layout.md#d531) | current |
| D532 | [android-drawing](decisions/android-drawing.md#d532) | current |
| D533 | [benchmarks](decisions/benchmarks.md#d533) | current |
| D534 | [android-drawing](decisions/android-drawing.md#d534) | current |
| D535 | [ui-and-layout](decisions/ui-and-layout.md#d535) | current |
| D536 | [android-host-and-performance](decisions/android-host-and-performance.md#d536) | current |
| D540 | [benchmarks](decisions/benchmarks.md#d540) | revised by [D622](decisions/api-and-tooling.md#d622) |
| D550 | [text](decisions/text.md#d550) | revised by [D554](decisions/text.md#d554), [D555](decisions/text.md#d555) |
| D551 | [text](decisions/text.md#d551) | current |
| D552 | [text](decisions/text.md#d552) | current |
| D553 | [benchmarks](decisions/benchmarks.md#d553) | current |
| D554 | [text](decisions/text.md#d554) | revised by [D556](decisions/text.md#d556), [D1200](decisions/ui-and-layout.md#d1200) |
| D555 | [text](decisions/text.md#d555) | current |
| D556 | [text](decisions/text.md#d556) | revised by [D1200](decisions/ui-and-layout.md#d1200), [D1201](decisions/ui-and-layout.md#d1201) |
| D560 | [lazy-and-prefetch](decisions/lazy-and-prefetch.md#d560) | revised by [D575](decisions/lazy-and-prefetch.md#d575), [D595](decisions/lazy-and-prefetch.md#d595) |
| D561 | [lazy-and-prefetch](decisions/lazy-and-prefetch.md#d561) | current |
| D562 | [lazy-and-prefetch](decisions/lazy-and-prefetch.md#d562) | current |
| D563 | [lazy-and-prefetch](decisions/lazy-and-prefetch.md#d563) | current |
| D564 | [lazy-and-prefetch](decisions/lazy-and-prefetch.md#d564) | current |
| D565 | [lazy-and-prefetch](decisions/lazy-and-prefetch.md#d565) | current |
| D566 | [android-drawing](decisions/android-drawing.md#d566) | current |
| D567 | [android-drawing](decisions/android-drawing.md#d567) | current |
| D568 | [android-host-and-performance](decisions/android-host-and-performance.md#d568) | revised by [D581](decisions/android-host-and-performance.md#d581) |
| D569 | [benchmarks](decisions/benchmarks.md#d569) | revised by [D574](decisions/benchmarks.md#d574), [D579](decisions/benchmarks.md#d579), [D601](decisions/android-host-and-performance.md#d601) |
| D570 | [ui-and-layout](decisions/ui-and-layout.md#d570) | current |
| D571 | [text](decisions/text.md#d571) | current |
| D572 | [ui-and-layout](decisions/ui-and-layout.md#d572) | current |
| D573 | [ui-and-layout](decisions/ui-and-layout.md#d573) | current |
| D574 | [benchmarks](decisions/benchmarks.md#d574) | revised by [D579](decisions/benchmarks.md#d579) |
| D575 | [lazy-and-prefetch](decisions/lazy-and-prefetch.md#d575) | revised by [D578](decisions/lazy-and-prefetch.md#d578) |
| D576 | [lazy-and-prefetch](decisions/lazy-and-prefetch.md#d576) | current |
| D577 | [lazy-and-prefetch](decisions/lazy-and-prefetch.md#d577) | current |
| D578 | [lazy-and-prefetch](decisions/lazy-and-prefetch.md#d578) | current |
| D579 | [benchmarks](decisions/benchmarks.md#d579) | current |
| D580 | [benchmarks](decisions/benchmarks.md#d580) | current |
| D581 | [android-host-and-performance](decisions/android-host-and-performance.md#d581) | revised by [D610](decisions/android-host-and-performance.md#d610) |
| D590 | [runtime](decisions/runtime.md#d590) | current |
| D591 | [runtime](decisions/runtime.md#d591) | current |
| D592 | [runtime](decisions/runtime.md#d592) | revised by [D597](decisions/runtime.md#d597) |
| D593 | [runtime](decisions/runtime.md#d593) | current |
| D594 | [runtime](decisions/runtime.md#d594) | current |
| D595 | [lazy-and-prefetch](decisions/lazy-and-prefetch.md#d595) | revised by [D597](decisions/runtime.md#d597), [D599](decisions/lazy-and-prefetch.md#d599) |
| D596 | [lazy-and-prefetch](decisions/lazy-and-prefetch.md#d596) | current |
| D597 | [runtime](decisions/runtime.md#d597) | current |
| D598 | [benchmarks](decisions/benchmarks.md#d598) | current |
| D599 | [lazy-and-prefetch](decisions/lazy-and-prefetch.md#d599) | current |
| D600 | [benchmarks](decisions/benchmarks.md#d600) | current |
| D601 | [android-host-and-performance](decisions/android-host-and-performance.md#d601) | revised by [D610](decisions/android-host-and-performance.md#d610) |
| D602 | [benchmarks](decisions/benchmarks.md#d602) | current |
| D603 | [benchmarks](decisions/benchmarks.md#d603) | current |
| D604 | [benchmarks](decisions/benchmarks.md#d604) | current |
| D605 | [benchmarks](decisions/benchmarks.md#d605) | current |
| D610 | [android-host-and-performance](decisions/android-host-and-performance.md#d610) | current |
| D620 | [api-and-tooling](decisions/api-and-tooling.md#d620) | current |
| D621 | [api-and-tooling](decisions/api-and-tooling.md#d621) | current |
| D622 | [api-and-tooling](decisions/api-and-tooling.md#d622) | current |
| D623 | [api-and-tooling](decisions/api-and-tooling.md#d623) | current |
| D630 | [ui-and-layout](decisions/ui-and-layout.md#d630) | current |
| D640 | [benchmarks](decisions/benchmarks.md#d640) | current |
| D641 | [benchmarks](decisions/benchmarks.md#d641) | current |
| D642 | [benchmarks](decisions/benchmarks.md#d642) | current |
| D643 | [benchmarks](decisions/benchmarks.md#d643) | current |
| D644 | [benchmarks](decisions/benchmarks.md#d644) | current |
| D645 | [benchmarks](decisions/benchmarks.md#d645) | current |
| D646 | [android-drawing](decisions/android-drawing.md#d646) | current |
| D647 | [benchmarks](decisions/benchmarks.md#d647) | revised by [D649](decisions/benchmarks.md#d649) |
| D648 | [android-drawing](decisions/android-drawing.md#d648) | current |
| D649 | [benchmarks](decisions/benchmarks.md#d649) | current |
| D660 | [benchmarks](decisions/benchmarks.md#d660) | current |
| D661 | [benchmarks](decisions/benchmarks.md#d661) | current |
| D700 | [api-and-tooling](decisions/api-and-tooling.md#d700) | current |
| D701 | [ui-and-layout](decisions/ui-and-layout.md#d701) | current |
| D702 | [runtime](decisions/runtime.md#d702) | current |
| D703 | [runtime](decisions/runtime.md#d703) | current |
| D704 | [ui-and-layout](decisions/ui-and-layout.md#d704) | current |
| D705 | [benchmarks](decisions/benchmarks.md#d705) | current |
| D710 | [api-and-tooling](decisions/api-and-tooling.md#d710) | current |
| D711 | [runtime](decisions/runtime.md#d711) | current |
| D712 | [runtime](decisions/runtime.md#d712) | current |
| D713 | [runtime](decisions/runtime.md#d713) | current |
| D714 | [runtime](decisions/runtime.md#d714) | current |
| D715 | [runtime](decisions/runtime.md#d715) | current |
| D716 | [runtime](decisions/runtime.md#d716) | current |
| D717 | [runtime](decisions/runtime.md#d717) | current |
| D800 | [ui-and-layout](decisions/ui-and-layout.md#d800) | revised by [D815](decisions/ui-and-layout.md#d815) |
| D801 | unused | (its test-clock rounding came in as [D900](decisions/api-and-tooling.md#d900)) |
| D802 | [runtime](decisions/runtime.md#d802) | revised by [D712](decisions/runtime.md#d712) |
| D803 | [ui-and-layout](decisions/ui-and-layout.md#d803) | current |
| D804 | [ui-and-layout](decisions/ui-and-layout.md#d804) | current |
| D805 | [ui-and-layout](decisions/ui-and-layout.md#d805) | current |
| D806 | [ui-and-layout](decisions/ui-and-layout.md#d806) | current |
| D807 | [ui-and-layout](decisions/ui-and-layout.md#d807) | current |
| D808 | [ui-and-layout](decisions/ui-and-layout.md#d808) | revised by [D815](decisions/ui-and-layout.md#d815) |
| D809 | [ui-and-layout](decisions/ui-and-layout.md#d809) | revised by [D816](decisions/ui-and-layout.md#d816) |
| D810 | [ui-and-layout](decisions/ui-and-layout.md#d810) | current |
| D811 | [ui-and-layout](decisions/ui-and-layout.md#d811) | current |
| D812 | [ui-and-layout](decisions/ui-and-layout.md#d812) | current |
| D813 | [ui-and-layout](decisions/ui-and-layout.md#d813) | revised by [D818](decisions/ui-and-layout.md#d818) |
| D814 | [ui-and-layout](decisions/ui-and-layout.md#d814) | current |
| D815 | [ui-and-layout](decisions/ui-and-layout.md#d815) | current |
| D816 | [ui-and-layout](decisions/ui-and-layout.md#d816) | current |
| D817 | [ui-and-layout](decisions/ui-and-layout.md#d817) | revised by [D824](decisions/ui-and-layout.md#d824) |
| D818 | [ui-and-layout](decisions/ui-and-layout.md#d818) | current |
| D819 | [ui-and-layout](decisions/ui-and-layout.md#d819) | revised by [D825](decisions/ui-and-layout.md#d825) |
| D820 | [api-and-tooling](decisions/api-and-tooling.md#d820) | current |
| D821 | [ui-and-layout](decisions/ui-and-layout.md#d821) | revised by [D823](decisions/ui-and-layout.md#d823) |
| D822 | [ui-and-layout](decisions/ui-and-layout.md#d822) | current |
| D823 | [ui-and-layout](decisions/ui-and-layout.md#d823) | current |
| D824 | [ui-and-layout](decisions/ui-and-layout.md#d824) | current |
| D825 | [ui-and-layout](decisions/ui-and-layout.md#d825) | current |
| D900 | [api-and-tooling](decisions/api-and-tooling.md#d900) | current |
| D901 | [api-and-tooling](decisions/api-and-tooling.md#d901) | current |
| D902 | [ui-and-layout](decisions/ui-and-layout.md#d902) | current |
| D903 | [api-and-tooling](decisions/api-and-tooling.md#d903) | revised by [D907](decisions/api-and-tooling.md#d907) |
| D904 | [api-and-tooling](decisions/api-and-tooling.md#d904) | current |
| D905 | [api-and-tooling](decisions/api-and-tooling.md#d905) | current |
| D906 | [api-and-tooling](decisions/api-and-tooling.md#d906) | current |
| D907 | [api-and-tooling](decisions/api-and-tooling.md#d907) | current |
| D908 | [api-and-tooling](decisions/api-and-tooling.md#d908) | current |
| D909 | [api-and-tooling](decisions/api-and-tooling.md#d909) | current |
| D1100 | [text](decisions/text.md#d1100) | revised by [D1112](decisions/text.md#d1112) |
| D1101 | [text](decisions/text.md#d1101) | current |
| D1102 | [text](decisions/text.md#d1102) | current |
| D1103 | [text](decisions/text.md#d1103) | current |
| D1104 | [text](decisions/text.md#d1104) | current |
| D1105 | [text](decisions/text.md#d1105) | revised by [D1108](decisions/text.md#d1108), [D1109](decisions/text.md#d1109), [D1110](decisions/text.md#d1110), [D1111](decisions/text.md#d1111) |
| D1106 | [text](decisions/text.md#d1106) | revised by [D1107](decisions/text.md#d1107) |
| D1107 | [text](decisions/text.md#d1107) | current |
| D1108 | [text](decisions/text.md#d1108) | revised by [D1114](decisions/text.md#d1114) |
| D1109 | [text](decisions/text.md#d1109) | current |
| D1110 | [text](decisions/text.md#d1110) | current |
| D1111 | [text](decisions/text.md#d1111) | current |
| D1112 | [text](decisions/text.md#d1112) | current |
| D1113 | [text](decisions/text.md#d1113) | reverted, to be redone in the IME step |
| D1114 | [text](decisions/text.md#d1114) | current |
| D1150 | [ui-and-layout](decisions/ui-and-layout.md#d1150) | current |
| D1151 | [ui-and-layout](decisions/ui-and-layout.md#d1151) | current |
| D1152 | [ui-and-layout](decisions/ui-and-layout.md#d1152) | current |
| D1153 | [ui-and-layout](decisions/ui-and-layout.md#d1153) | current |
| D1154 | [ui-and-layout](decisions/ui-and-layout.md#d1154) | current |
| D1155 | [ui-and-layout](decisions/ui-and-layout.md#d1155) | current |
| D1156 | [ui-and-layout](decisions/ui-and-layout.md#d1156) | current |
| D1157 | [ui-and-layout](decisions/ui-and-layout.md#d1157) | revised by [D1161](decisions/ui-and-layout.md#d1161) |
| D1158 | [ui-and-layout](decisions/ui-and-layout.md#d1158) | current |
| D1159 | [ui-and-layout](decisions/ui-and-layout.md#d1159) | current |
| D1160 | [ui-and-layout](decisions/ui-and-layout.md#d1160) | current |
| D1161 | [ui-and-layout](decisions/ui-and-layout.md#d1161) | current |
| D1162 | [ui-and-layout](decisions/ui-and-layout.md#d1162) | current |
| D1163 | [ui-and-layout](decisions/ui-and-layout.md#d1163) | current |
| D1200 | [ui-and-layout](decisions/ui-and-layout.md#d1200) | current |
| D1201 | [ui-and-layout](decisions/ui-and-layout.md#d1201) | current |
| D1202 | [ui-and-layout](decisions/ui-and-layout.md#d1202) | current |
| D1203 | [ui-and-layout](decisions/ui-and-layout.md#d1203) | current |
| D1204 | [ui-and-layout](decisions/ui-and-layout.md#d1204) | current |
| D1205 | [text](decisions/text.md#d1205) | current |
| D1206 | [ui-and-layout](decisions/ui-and-layout.md#d1206) | current |
| D1207 | [ui-and-layout](decisions/ui-and-layout.md#d1207) | current |
| D1208 | [ui-and-layout](decisions/ui-and-layout.md#d1208) | current |
| D1250 | [ui-and-layout](decisions/ui-and-layout.md#d1250) | current |
| D1251 | [ui-and-layout](decisions/ui-and-layout.md#d1251) | current |
| D1252 | [ui-and-layout](decisions/ui-and-layout.md#d1252) | revised by [D1255](decisions/ui-and-layout.md#d1255) |
| D1253 | [ui-and-layout](decisions/ui-and-layout.md#d1253) | revised by [D1255](decisions/ui-and-layout.md#d1255) |
| D1254 | [ui-and-layout](decisions/ui-and-layout.md#d1254) | current |
| D1255 | [ui-and-layout](decisions/ui-and-layout.md#d1255) | current |
| D1256 | [ui-and-layout](decisions/ui-and-layout.md#d1256) | current |
