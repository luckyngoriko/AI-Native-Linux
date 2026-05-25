//! S15.2 graph evaluator layered over the S15.1 [`ServiceGraph`] surface.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use crate::{
    DependencyEdge, DependencyKind, DesiredState, GraphState, ServiceGraph, ServiceUnit, SgrError,
    SgrEvidenceEmitter, UnitId, UnitState,
};

/// Deterministic graph evaluator for dependency solving and convergence checks.
pub struct GraphEvaluator {
    graph: Arc<dyn ServiceGraph>,
    evidence_emitter: Option<Arc<SgrEvidenceEmitter>>,
}

impl GraphEvaluator {
    /// Construct a graph evaluator over a service graph implementation.
    #[must_use]
    pub fn new(graph: Arc<dyn ServiceGraph>) -> Self {
        Self {
            graph,
            evidence_emitter: None,
        }
    }

    /// Construct a graph evaluator with evidence emission enabled.
    #[must_use]
    pub fn with_evidence_emitter(
        graph: Arc<dyn ServiceGraph>,
        evidence_emitter: Arc<SgrEvidenceEmitter>,
    ) -> Self {
        Self {
            graph,
            evidence_emitter: Some(evidence_emitter),
        }
    }

    /// Return units in dependency order using Kahn's algorithm.
    ///
    /// A unit appears only after every declared `Requires*` / `OrdersAfter`
    /// prerequisite has appeared. Alphabetical `unit_id` tie-breaking keeps the
    /// order deterministic per S15.2 §5.1.
    ///
    /// # Errors
    ///
    /// Returns [`SgrError::DependencyCycleDetected`] when the dependency graph
    /// contains a cycle, or propagates the underlying [`ServiceGraph`] error.
    pub async fn topological_sort(&self) -> Result<Vec<UnitId>, SgrError> {
        let index = self.load_dependency_index().await?;
        let mut prerequisite_counts = index.prerequisite_counts.clone();
        let mut ready = zero_prerequisite_units(&index.units, &prerequisite_counts);
        let mut ordered = Vec::with_capacity(index.units.len());

        while !ready.is_empty() {
            let unit_id = ready.remove(0);
            ordered.push(unit_id.clone());

            for dependent in index.dependents.get(&unit_id).into_iter().flatten() {
                decrement_prerequisite_count(&mut prerequisite_counts, dependent)?;
                if prerequisite_counts
                    .get(dependent)
                    .copied()
                    .unwrap_or_default()
                    == 0
                {
                    ready.push(dependent.clone());
                }
            }
            sort_and_dedup_unit_ids(&mut ready);
        }

        if ordered.len() != index.units.len() {
            return Err(SgrError::DependencyCycleDetected(first_cycle_or_remaining(
                &index, &ordered,
            )?));
        }

        Ok(ordered)
    }

    /// Detect all dependency cycles using an iterative Tarjan SCC walk.
    ///
    /// The returned components are in the reverse topological order produced by
    /// Tarjan's root-pop sequence. Components are filtered to real cycles:
    /// size >= 2, or a single-node SCC with an explicit self-loop.
    ///
    /// # Errors
    ///
    /// Propagates the underlying [`ServiceGraph`] error.
    pub async fn detect_cycles(&self) -> Result<Vec<Vec<UnitId>>, SgrError> {
        let index = self.load_dependency_index().await?;
        detect_cycles_in_index(&index)
    }

    /// Partition the graph into dependency levels that can start concurrently.
    ///
    /// # Errors
    ///
    /// Returns [`SgrError::DependencyCycleDetected`] when the graph is cyclic,
    /// or propagates the underlying [`ServiceGraph`] error.
    pub async fn identify_parallel_batches(&self) -> Result<Vec<Vec<UnitId>>, SgrError> {
        let index = self.load_dependency_index().await?;
        let mut prerequisite_counts = index.prerequisite_counts.clone();
        let mut ready = zero_prerequisite_units(&index.units, &prerequisite_counts);
        let mut batches = Vec::new();
        let mut processed = Vec::with_capacity(index.units.len());

        while !ready.is_empty() {
            let batch = ready;
            let mut next = Vec::new();

            for unit_id in &batch {
                processed.push(unit_id.clone());
                for dependent in index.dependents.get(unit_id).into_iter().flatten() {
                    decrement_prerequisite_count(&mut prerequisite_counts, dependent)?;
                    if prerequisite_counts
                        .get(dependent)
                        .copied()
                        .unwrap_or_default()
                        == 0
                    {
                        next.push(dependent.clone());
                    }
                }
            }

            sort_and_dedup_unit_ids(&mut next);
            batches.push(batch);
            ready = next;
        }

        if processed.len() != index.units.len() {
            return Err(SgrError::DependencyCycleDetected(first_cycle_or_remaining(
                &index, &processed,
            )?));
        }

        Ok(batches)
    }

    /// Return true when all hard `Requires*` dependencies are running-stable.
    ///
    /// `OrdersAfter` edges affect ordering but do not block readiness.
    ///
    /// # Errors
    ///
    /// Returns [`SgrError::UnitNotFound`] when `unit_id` or a dependency target
    /// is absent.
    pub async fn evaluate_readiness(&self, unit_id: &UnitId) -> Result<bool, SgrError> {
        for edge in self.graph.list_dependencies(unit_id).await? {
            if edge.kind.is_hard() {
                let dependency = self.graph.get_unit(&edge.to_unit_id).await?;
                if !is_running_stable(dependency.state) {
                    return Ok(false);
                }
            }
        }

        Ok(true)
    }

    /// Return true when every unit has reached its requested stable state.
    ///
    /// Empty graphs are converged by definition.
    ///
    /// # Errors
    ///
    /// Propagates the underlying [`ServiceGraph`] error.
    pub async fn is_converged(&self) -> Result<bool, SgrError> {
        let units = self.graph.list_units().await?;
        Ok(units.iter().all(|unit| {
            !is_transitional(unit.state)
                && desired_state_matches(unit.manifest.desired_state, unit.state)
        }))
    }

    /// Derive the graph convergence state from current unit states and desired states.
    ///
    /// # Errors
    ///
    /// Propagates the underlying [`ServiceGraph`] error.
    pub async fn convergence_state(&self) -> Result<GraphState, SgrError> {
        let units = self.graph.list_units().await?;
        let state = if units.is_empty() {
            GraphState::Empty
        } else if units.iter().any(|unit| unit.state == UnitState::Failed) {
            let failed_count = units
                .iter()
                .filter(|unit| unit.state == UnitState::Failed)
                .count();
            let critical_failed = units
                .iter()
                .any(|unit| unit.state == UnitState::Failed && is_critical_unit(unit));
            if critical_failed || failed_count == units.len() {
                GraphState::Failed
            } else {
                GraphState::Degraded
            }
        } else if units.iter().any(|unit| {
            matches!(
                unit.state,
                UnitState::Degraded | UnitState::Unhealthy | UnitState::Retired
            )
        }) {
            GraphState::Degraded
        } else if units.iter().any(|unit| is_transitional(unit.state)) {
            GraphState::Converging
        } else if units
            .iter()
            .all(|unit| desired_state_matches(unit.manifest.desired_state, unit.state))
        {
            GraphState::Converged
        } else {
            GraphState::Converging
        };

        if state == GraphState::Converged {
            if let Some(emitter) = &self.evidence_emitter {
                let unit_count = u64::try_from(units.len()).map_err(|err| {
                    SgrError::Internal(format!("unit count conversion failed: {err}"))
                })?;
                emitter
                    .emit_graph_converged(state, unit_count, None)
                    .await?;
            }
        }

        Ok(state)
    }

    async fn load_dependency_index(&self) -> Result<DependencyIndex, SgrError> {
        let mut units = self
            .graph
            .list_units()
            .await?
            .into_iter()
            .map(|unit| unit.unit_id)
            .collect::<Vec<_>>();
        sort_unit_ids(&mut units);

        let unit_set = units.iter().cloned().collect::<HashSet<_>>();
        let mut adjacency = units
            .iter()
            .cloned()
            .map(|unit_id| (unit_id, Vec::new()))
            .collect::<HashMap<_, _>>();
        let mut dependents = adjacency.clone();
        let mut prerequisite_counts = units
            .iter()
            .cloned()
            .map(|unit_id| (unit_id, 0usize))
            .collect::<HashMap<_, _>>();
        let mut self_loops = HashSet::new();

        for unit_id in &units {
            let mut edges = self.graph.list_dependencies(unit_id).await?;
            sort_dependency_edges(&mut edges);

            for edge in edges {
                if edge.from_unit_id.as_str() != unit_id.as_str() {
                    return Err(SgrError::Internal(format!(
                        "dependency edge source mismatch: expected {unit_id}, got {}",
                        edge.from_unit_id
                    )));
                }
                if !unit_set.contains(&edge.to_unit_id) {
                    return Err(SgrError::DependencyTargetNotRegistered(edge.to_unit_id));
                }

                adjacency
                    .get_mut(unit_id)
                    .ok_or_else(|| missing_index_entry("adjacency", unit_id))?
                    .push(edge.to_unit_id.clone());
                dependents
                    .get_mut(&edge.to_unit_id)
                    .ok_or_else(|| missing_index_entry("dependents", &edge.to_unit_id))?
                    .push(unit_id.clone());
                *prerequisite_counts
                    .get_mut(unit_id)
                    .ok_or_else(|| missing_index_entry("prerequisite count", unit_id))? += 1;

                if edge.to_unit_id.as_str() == unit_id.as_str() {
                    self_loops.insert(unit_id.clone());
                }
            }
        }

        for neighbors in adjacency.values_mut() {
            sort_unit_ids(neighbors);
        }
        for neighbors in dependents.values_mut() {
            sort_unit_ids(neighbors);
        }

        Ok(DependencyIndex {
            units,
            adjacency,
            dependents,
            prerequisite_counts,
            self_loops,
        })
    }
}

#[derive(Debug)]
struct DependencyIndex {
    units: Vec<UnitId>,
    adjacency: HashMap<UnitId, Vec<UnitId>>,
    dependents: HashMap<UnitId, Vec<UnitId>>,
    prerequisite_counts: HashMap<UnitId, usize>,
    self_loops: HashSet<UnitId>,
}

#[derive(Debug)]
struct TarjanFrame {
    node: UnitId,
    next_neighbor: usize,
}

fn detect_cycles_in_index(index: &DependencyIndex) -> Result<Vec<Vec<UnitId>>, SgrError> {
    let mut next_index = 0usize;
    let mut indexes = HashMap::new();
    let mut lowlinks = HashMap::new();
    let mut component_stack = Vec::new();
    let mut on_stack = HashSet::new();
    let mut components = Vec::new();

    for start in &index.units {
        if indexes.contains_key(start) {
            continue;
        }

        push_tarjan_node(
            start.clone(),
            &mut next_index,
            &mut indexes,
            &mut lowlinks,
            &mut component_stack,
            &mut on_stack,
        );
        let mut frames = vec![TarjanFrame {
            node: start.clone(),
            next_neighbor: 0,
        }];

        while !frames.is_empty() {
            let (node, maybe_neighbor) = {
                let frame = frames
                    .last_mut()
                    .ok_or_else(|| SgrError::Internal("empty Tarjan frame stack".to_owned()))?;
                let node = frame.node.clone();
                let neighbors = index
                    .adjacency
                    .get(&node)
                    .ok_or_else(|| missing_index_entry("adjacency", &node))?;

                if frame.next_neighbor < neighbors.len() {
                    let neighbor = neighbors[frame.next_neighbor].clone();
                    frame.next_neighbor += 1;
                    (node, Some(neighbor))
                } else {
                    (node, None)
                }
            };

            if let Some(neighbor) = maybe_neighbor {
                if !indexes.contains_key(&neighbor) {
                    push_tarjan_node(
                        neighbor.clone(),
                        &mut next_index,
                        &mut indexes,
                        &mut lowlinks,
                        &mut component_stack,
                        &mut on_stack,
                    );
                    frames.push(TarjanFrame {
                        node: neighbor,
                        next_neighbor: 0,
                    });
                } else if on_stack.contains(&neighbor) {
                    let neighbor_index = tarjan_value(&indexes, &neighbor, "index")?;
                    lower_lowlink(&mut lowlinks, &node, neighbor_index)?;
                }
                continue;
            }

            if tarjan_value(&lowlinks, &node, "lowlink")? == tarjan_value(&indexes, &node, "index")?
            {
                components.push(pop_component(&mut component_stack, &mut on_stack, &node)?);
            }

            frames.pop();
            if let Some(parent) = frames.last() {
                let child_lowlink = tarjan_value(&lowlinks, &node, "lowlink")?;
                lower_lowlink(&mut lowlinks, &parent.node, child_lowlink)?;
            }
        }
    }

    Ok(components
        .into_iter()
        .filter(|component| {
            component.len() >= 2
                || component
                    .first()
                    .is_some_and(|unit_id| index.self_loops.contains(unit_id))
        })
        .collect())
}

fn push_tarjan_node(
    node: UnitId,
    next_index: &mut usize,
    indexes: &mut HashMap<UnitId, usize>,
    lowlinks: &mut HashMap<UnitId, usize>,
    component_stack: &mut Vec<UnitId>,
    on_stack: &mut HashSet<UnitId>,
) {
    let index = *next_index;
    *next_index += 1;
    indexes.insert(node.clone(), index);
    lowlinks.insert(node.clone(), index);
    component_stack.push(node.clone());
    on_stack.insert(node);
}

fn pop_component(
    component_stack: &mut Vec<UnitId>,
    on_stack: &mut HashSet<UnitId>,
    root: &UnitId,
) -> Result<Vec<UnitId>, SgrError> {
    let mut component = Vec::new();

    loop {
        let node = component_stack
            .pop()
            .ok_or_else(|| SgrError::Internal("empty Tarjan component stack".to_owned()))?;
        on_stack.remove(&node);
        let is_root = &node == root;
        component.push(node);
        if is_root {
            break;
        }
    }

    sort_unit_ids(&mut component);
    Ok(component)
}

fn first_cycle_or_remaining(
    index: &DependencyIndex,
    processed: &[UnitId],
) -> Result<Vec<UnitId>, SgrError> {
    let cycles = detect_cycles_in_index(index)?;
    if let Some(cycle) = cycles.into_iter().next() {
        return Ok(cycle);
    }

    let processed = processed.iter().cloned().collect::<HashSet<_>>();
    let mut remaining = index
        .units
        .iter()
        .filter(|unit_id| !processed.contains(*unit_id))
        .cloned()
        .collect::<Vec<_>>();
    sort_unit_ids(&mut remaining);
    Ok(remaining)
}

fn zero_prerequisite_units(
    units: &[UnitId],
    prerequisite_counts: &HashMap<UnitId, usize>,
) -> Vec<UnitId> {
    let mut ready = units
        .iter()
        .filter(|unit_id| {
            prerequisite_counts
                .get(*unit_id)
                .copied()
                .unwrap_or_default()
                == 0
        })
        .cloned()
        .collect::<Vec<_>>();
    sort_unit_ids(&mut ready);
    ready
}

fn decrement_prerequisite_count(
    prerequisite_counts: &mut HashMap<UnitId, usize>,
    unit_id: &UnitId,
) -> Result<(), SgrError> {
    let count = prerequisite_counts
        .get_mut(unit_id)
        .ok_or_else(|| missing_index_entry("prerequisite count", unit_id))?;
    if *count == 0 {
        return Err(SgrError::Internal(format!(
            "dependency count underflow for {unit_id}"
        )));
    }
    *count -= 1;
    Ok(())
}

fn tarjan_value(
    values: &HashMap<UnitId, usize>,
    unit_id: &UnitId,
    label: &str,
) -> Result<usize, SgrError> {
    values
        .get(unit_id)
        .copied()
        .ok_or_else(|| missing_index_entry(label, unit_id))
}

fn lower_lowlink(
    lowlinks: &mut HashMap<UnitId, usize>,
    unit_id: &UnitId,
    candidate: usize,
) -> Result<(), SgrError> {
    let current = lowlinks
        .get_mut(unit_id)
        .ok_or_else(|| missing_index_entry("lowlink", unit_id))?;
    if candidate < *current {
        *current = candidate;
    }
    Ok(())
}

fn sort_dependency_edges(edges: &mut [DependencyEdge]) {
    edges.sort_by(|left, right| {
        left.to_unit_id
            .as_str()
            .cmp(right.to_unit_id.as_str())
            .then_with(|| dependency_kind_rank(left.kind).cmp(&dependency_kind_rank(right.kind)))
    });
}

const fn dependency_kind_rank(kind: DependencyKind) -> u8 {
    match kind {
        DependencyKind::RequiresHealthy => 0,
        DependencyKind::RequiresRunning => 1,
        DependencyKind::OrdersAfter => 2,
    }
}

fn sort_and_dedup_unit_ids(unit_ids: &mut Vec<UnitId>) {
    sort_unit_ids(unit_ids);
    unit_ids.dedup();
}

fn sort_unit_ids(unit_ids: &mut [UnitId]) {
    unit_ids.sort_by(|left, right| left.as_str().cmp(right.as_str()));
}

const fn is_running_stable(state: UnitState) -> bool {
    matches!(state, UnitState::Running | UnitState::Healthy)
}

const fn is_transitional(state: UnitState) -> bool {
    matches!(
        state,
        UnitState::Draft | UnitState::Queued | UnitState::Starting | UnitState::Stopping
    )
}

const fn desired_state_matches(desired_state: DesiredState, unit_state: UnitState) -> bool {
    match desired_state {
        DesiredState::Running | DesiredState::Restarted | DesiredState::Reloaded => {
            is_running_stable(unit_state)
        }
        DesiredState::Stopped => matches!(unit_state, UnitState::Stopped),
    }
}

fn is_critical_unit(unit: &ServiceUnit) -> bool {
    unit.manifest
        .labels
        .as_ref()
        .and_then(|labels| labels.get("criticality"))
        .and_then(serde_json::Value::as_str)
        .is_some_and(|value| value.eq_ignore_ascii_case("critical"))
}

fn missing_index_entry(label: &str, unit_id: &UnitId) -> SgrError {
    SgrError::Internal(format!("missing {label} for {unit_id}"))
}
