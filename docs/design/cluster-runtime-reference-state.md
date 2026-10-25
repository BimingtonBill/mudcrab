# Runtime enable state for distant static references

Cluster coverage deliberately refuses references whose enable state can change. A refusal is
not permission to omit their geometry outside the loaded grid. In cluster-owned mode,
model-bearing STAT and TREE references with look or runtime refusals use the ordinary source
loader at every distance when enabled. Dynamic record types retain their existing grid policy.

## State ownership and inputs

`CertifiedPackage` reads an immutable enable catalog under its retained source-data lock.
The catalog contains winning reference inputs and reverse parent dependencies, including
model-less parents outside the drawn worldspace. It reuses the ordinary database loader's
strict metadata decoder and initial enable resolver. Reading metadata does not load cells,
models, or terrain. Retaining the catalog and its resolved session cache costs memory once,
in exchange for resolving state without database reads or scene scans during gameplay.

A parent controls the initial child state; XESP bit zero reverses that state. The child's
initially-disabled flag applies when it has no parent. Missing and cyclic parent chains stay
unresolved and generate diagnostics. Legacy databases without enable columns retain the
existing, explicitly warned default-enabled compatibility policy; partial metadata fails.

Gameplay changes state through `world.trigger(SetReferenceEnabled { reference, enabled })`.
The key is the placed reference ID, not a base object or mesh. An explicit session override
anchors that reference's state. Descendants follow it unless they have their own explicit
override. Repeating the same effective state does no resolution or visibility work. Changes
and their nested observer commands are applied through Bevy's normal command flush points.

The observer is installed for ordinary world streaming as well as cluster mode. The API requires
a certified cluster enable catalog: before the first bind, it queues the last request per reference.
Binding applies explicit parent and child overrides together, then reconciles live roots and
publishes effective changes to cluster suppression. The first writer call warns once about scope.
Indexed live roots can be hidden and restored after binding, including interiors. Creating a
missing stock body is supported only for cluster-owned exterior refused static scenery. Other
missing bodies require an ordinary cell commit; a non-cluster run without a catalog cannot apply
queued state. This is an explicit boundary, not a general gameplay spawn or script implementation.

Visibility writers use the enable gate as the authority and retain their latest requested policy
while disabled, so restoring enable state does not resurrect an obsolete portal or light policy.

Equal winning inputs preserve overrides across recertification, even when a new package owns
the catalog allocation. Changed inputs reset the session cache and reconcile indexed surviving
roots once. Entity unload and reload do not clear overrides.

## Drawing and invalidation

Far planning is keyed on demand, coverage feedback, and the enable-state revision. Its existing
settled early return prevents a state inventory scan when nothing changes. Sparse publication
and ordinary near-cell commits resolve current state again before spawning; an old database
response cannot overwrite a runtime enable decision.

An effective change updates indexed reference roots and gates their actual source subtrees,
including unparented flat draws. Temporary gates preserve authored visibility, update inherited
visibility even when a root carries Bevy's `Disabled` component, and cover late scene insertion
and reparenting. Flat synchronization respects the gate. These operations are driven by state
and component lifecycle events, with no per-frame enable-state system.

Enable suppression of cluster instances is reversible. Pose changes and generic
`ReferenceStateChanged` revocations remain permanent for the session; enabling a reference
cannot erase them. The existing partial-source selector remains authoritative: a partly owned
reference instantiates only its refused source occurrences, while a wholly refused reference
uses the ordinary complete model. Enabling does not recreate owned geometry as stock.

Reversible cluster suppression is projected through the certificate's mapped-reference index,
including partly mapped and not-yet-resident references. Toggling a wholly unclustered reference
still gates its stock roots, but does not invalidate cluster selection masks or the readiness
catalog. Replacing the certificate reprojects retained suppression even if enable inputs match.

## Diagnostics and limits

The existing `cluster/undrawn_static_geometry` and `cluster/undrawn_runtime_refused` counters
retain their first-qualified snapshot contract, once per package/worldspace. Known disabled
runtime references no longer contribute to the missing-route count. Unresolved runtime state
is conservatively reported as a missing route rather than being called disabled.

`cluster/runtime_refused_hidden_by_state` and `cluster/runtime_refused_unresolved_state` are
sampled gauges for model-bearing, runtime-refused static scenery beyond the loaded grid
at the snapshot's center.
Inventory snapshots are taken at first world qualification and when effective enable state
changes relative to that world's last sample. Returning to a world restores its own cached
gauges, refreshing once if state changed while away. Demand-only cell crossings perform zero inventory walks; the cumulative
`cluster/stock_inventory_walks` counter measures this work. The inventory diagnoses route selection; it does
not certify completed asset IO or render readiness. A complete state-coverage claim also
requires the unresolved-state gauge to be zero.

Initial values are the existing new-game bootstrap, not imported save or quest state. Runtime
writers must use the enable event for reversible changes and trigger the generic state event
for other representation changes. Retail save and script parity is not established by this
draw-path implementation.
