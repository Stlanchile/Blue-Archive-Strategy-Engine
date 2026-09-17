# ADR 0002: Marginal acquisition timing outside the Markov state

Status: accepted for v0.4.

Ownership is monotone. An unset target bit proves no prior acquisition on that
path. Observe `post_mask & !pre_mask` on the existing categorical primitive
transition and accumulate its already-computed child mass at that draw count.
Initially owned targets contribute time zero. Terminal unowned mass is folded
independently. Thus histories with different first acquisition times can merge
into the same future-relevant state without losing the marginal report.

Keep WorldStateKey and InFlightStateKey unchanged. Exact uses private sparse
ScaledMass maps; concrete simulation keeps at most four optional first counts,
then checked integer histograms. Collection never affects strategy, kernel,
propagation order, randomness, terminal handling or legacy arithmetic. A private
enabled/disabled path suffices; no public observer interface is introduced.

Opt-in result schema 4 wraps the unchanged schema-3 analysis. Document schemas,
engine semantics, canonical encoding, run-stream derivation and trace/replay
remain unchanged. Comparison executes each solver once and aligns sparse
support plus zero/horizon. Storage and rendering receive independent fixed caps.

This does not produce joint timing distributions, optimization or conditional
statistics. An early acquisition within an atomic ticket action remains an
observation at the primitive draw, not a new strategy decision boundary.
