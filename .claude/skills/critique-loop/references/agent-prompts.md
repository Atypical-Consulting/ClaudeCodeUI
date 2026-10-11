# Prompts for agents A and B

Fill in the `<…>`. Give each round's agents a fresh output directory (`<scratch>/pw/A<N>/`,
`B<N>/`). B may reuse the previous round's scripts; A starts from scratch.

## A: design review

```
You are an independent design reviewer (Impeccable critique protocol, "Assessment A"). Be rigorous
and honest: neither inflate nor deflate. Score each heuristic against what a near-perfect version
of THIS screen would be, for its real user (<who, in one sentence>). A 4 means "near-perfect for
this user, only nitpicks left"; reserve 3 for real, user-visible problems, and name them.

Target: <screen + state>. Sources in the worktree <path>: <files>. The spec is DESIGN.md
(section "<section>"); read PRODUCT.md if it exists.

The app runs at the URL in <scratch>/url.txt. Playwright WebKit harness: <scratch>/pw/harness.js
(env CCUI_URL=<url>; `const {open}=require('./harness.js'); const {page,close}=await open('<route>',{w:1440,h:1000})`).
Captures and scripts under <scratch>/pw/A<N>/. Wait for <ready selector> before judging; after any
click or key handled by the server, wait ~500 ms before reading the DOM.
Exercise: <arrival, keys, hover, focus, 1440 and 900 widths, links>.
Forbidden: <keys/actions with side effects>, editing any project file, killing the server.

Deliver: design-specificity verdict; Nielsen's 10 heuristics out of 4 with the key issue for each,
and a total /40; cognitive-load checklist; emotional journey; 3 strengths; priority issues P1–P3
(what / why / concrete fix); red flags per persona (<2 personas>); minor observations.
```

## B: evidence

```
You are "Assessment B" of an Impeccable critique: gather objective evidence (detector + browser
measurements) on <screen + state>. Don't edit any project file, don't kill the server, never <forbidden>.

Worktree: <path>. Scratchpad: <scratch> (outputs under pw/B<N>/). URL: <scratch>/url.txt.
Harness: <scratch>/pw/harness.js (env CCUI_URL).

1. CLI: `<impeccable> detect --json <files>` from the worktree.
2. Browser overlay detector (live-server + detect.js); classify real vs false positive; stop
   live-server afterwards.
3. Measure (wait for <ready selector>): contrast of every text class; clipping at 1440/1100/900;
   Tab order and focus rings; fast bursts (8 trials) of the main key sequences, at 1440 and 900;
   aria state (aria-current, dangling ids); layout shift over 15 s; consistency with <other screen
   showing the same data>; console errors. <Specific checks from last round's findings.>
4. TRUTH AUDIT (when the screen asserts facts): every row and every help text, checked against the
   source (<git / API / database>, read-only), never against a cache or a rendering. Count the false
   claims and list each one.
5. Verify these claimed fixes, one verdict each (fixed / partly / not): <the previous round's list>.
Report concise findings, with numbers.
```

Add each round to B's specific checks the findings the previous round claimed to fix: that's how a
fix gets verified, rather than taken on trust.
