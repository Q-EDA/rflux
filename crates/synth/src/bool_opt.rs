use std::collections::{BTreeMap, HashMap, VecDeque};

use rflux_ir::{LogicOp, Netlist, NodeId, NodeKind, PinRef};

use crate::{
    BoolOptCompatibilityIssue, BoolOptCompatibilityIssueKind, BoolOptCompatibilityReport,
    BoolOptConfig, BoolOptReport, Compiler, SynthError,
};

#[derive(Debug, Clone)]
enum LogicExprKind {
    Input,
    Not(usize),
    And(Vec<usize>),
    Or(Vec<usize>),
    Xor(Vec<usize>),
    Mux2([usize; 3]),
    DffEnable([usize; 3]),
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
enum LogicExprKey {
    Not(usize),
    And(Vec<usize>),
    Or(Vec<usize>),
    Xor(Vec<usize>),
    Mux2([usize; 3]),
    DffEnable([usize; 3]),
}

#[derive(Debug, Clone)]
struct LogicExpr {
    kind: LogicExprKind,
    node_kind: NodeKind,
    name: String,
    logic_op: Option<LogicOp>,
}

impl Compiler {
    pub fn optimize_boolean_network(
        &mut self,
        netlist: &mut Netlist,
        config: &BoolOptConfig,
    ) -> BoolOptReport {
        let gate_count_before = count_logic_gates(netlist);

        if let Ok(Some(normalized)) = normalize_mux_feedback_dffe(netlist) {
            *netlist = normalized;
        }

        let compatibility = self.analyze_bool_opt_compatibility(netlist);

        if !compatibility.is_compatible() {
            return BoolOptReport {
                gate_count_before,
                gate_count_after: gate_count_before,
            };
        }

        match self.rebuild_optimized_boolean_network(netlist, config) {
            Ok(rewritten) => {
                let gate_count_after = count_logic_gates(&rewritten);
                *netlist = rewritten;
                BoolOptReport {
                    gate_count_before,
                    gate_count_after,
                }
            }
            Err(_) => BoolOptReport {
                gate_count_before,
                gate_count_after: gate_count_before,
            },
        }
    }

    pub fn analyze_bool_opt_compatibility(&self, netlist: &Netlist) -> BoolOptCompatibilityReport {
        let (mut incoming_by_node, outdegree) = incoming_and_outdegree(netlist);
        for incoming in &mut incoming_by_node {
            incoming.sort_by_key(|(port, _)| *port);
        }

        let mut report = BoolOptCompatibilityReport::default();
        let mut resolved = vec![false; netlist.node_count()];

        for node in netlist.nodes() {
            if outdegree[node.id.0] == 0 {
                report.output_candidates.push(node.id.0);
            }

            if incoming_by_node[node.id.0].is_empty() {
                match node.kind {
                    NodeKind::Port => {
                        report.input_nodes.push(node.id.0);
                        resolved[node.id.0] = true;
                    }
                    NodeKind::CellInstance
                    | NodeKind::MacroCell
                    | NodeKind::Splitter
                    | NodeKind::Jtl
                    | NodeKind::Ptl
                    | NodeKind::Dff => report.issues.push(BoolOptCompatibilityIssue {
                        node: node.id.0,
                        kind: BoolOptCompatibilityIssueKind::MissingDriver,
                        detail: "node has no incoming driver".to_string(),
                    }),
                }
                continue;
            }

            match node.kind {
                NodeKind::Port | NodeKind::Splitter | NodeKind::Jtl | NodeKind::Ptl => {
                    expect_exact_inputs(
                        &mut report,
                        node.id.0,
                        incoming_by_node[node.id.0].len(),
                        1,
                    );
                }
                NodeKind::Dff => match node.logic_op {
                    Some(LogicOp::DffEnable) => {
                        expect_exact_inputs(
                            &mut report,
                            node.id.0,
                            incoming_by_node[node.id.0].len(),
                            3,
                        );
                    }
                    _ => report.issues.push(BoolOptCompatibilityIssue {
                        node: node.id.0,
                        kind: BoolOptCompatibilityIssueKind::UnsupportedNodeKind,
                        detail: "sequential Dff optimization only supports DffEnable".to_string(),
                    }),
                },
                NodeKind::CellInstance | NodeKind::MacroCell => {
                    if let Some(expected) = expected_logic_inputs(node) {
                        expect_exact_inputs(
                            &mut report,
                            node.id.0,
                            incoming_by_node[node.id.0].len(),
                            expected,
                        );
                    }
                }
            }
        }

        let mut progress = true;
        while progress {
            progress = false;
            for node in netlist.nodes() {
                if resolved[node.id.0] {
                    continue;
                }

                let incoming = &incoming_by_node[node.id.0];
                if incoming.is_empty() {
                    continue;
                }

                if incoming.iter().all(|(_, source)| resolved[source.node.0]) {
                    resolved[node.id.0] = true;
                    progress = true;
                }
            }
        }

        for node in netlist.nodes() {
            if !resolved[node.id.0] && !report.issues.iter().any(|issue| issue.node == node.id.0) {
                report.issues.push(BoolOptCompatibilityIssue {
                    node: node.id.0,
                    kind: BoolOptCompatibilityIssueKind::CycleOrUnresolvedDependency,
                    detail: "node dependencies could not be resolved in combinational order"
                        .to_string(),
                });
            }
        }

        report
    }

    fn rebuild_optimized_boolean_network(
        &mut self,
        original: &Netlist,
        config: &BoolOptConfig,
    ) -> Result<Netlist, SynthError> {
        let topo_order = topological_order(original)?;
        let (mut incoming_by_node, outdegree) = incoming_and_outdegree(original);
        for incoming in &mut incoming_by_node {
            incoming.sort_by_key(|(port, _)| *port);
        }

        let mut exprs = Vec::<LogicExpr>::new();
        let mut expr_of_node = vec![None::<usize>; original.node_count()];
        let mut logic_exprs = BTreeMap::<LogicExprKey, usize>::new();

        for node_index in topo_order {
            let node = &original.nodes()[node_index];
            let incoming = &incoming_by_node[node_index];

            if incoming.is_empty() {
                match node.kind {
                    NodeKind::Port => {
                        expr_of_node[node_index] = Some(exprs.len());
                        exprs.push(LogicExpr {
                            kind: LogicExprKind::Input,
                            node_kind: NodeKind::Port,
                            name: node.name.clone(),
                            logic_op: None,
                        });
                    }
                    _ => return Err(SynthError::MissingBoolOptDriver(node_index)),
                }
                continue;
            }

            let mut operands = Vec::with_capacity(incoming.len());
            for (_, source) in incoming {
                operands.push(
                    expr_of_node[source.node.0].ok_or(SynthError::CycleOrUnsupportedDependency)?,
                );
            }

            let expr_id = match node.kind {
                NodeKind::Port | NodeKind::Splitter | NodeKind::Jtl | NodeKind::Ptl => {
                    ensure_input_count(node_index, operands.len(), 1)?;
                    operands[0]
                }
                NodeKind::CellInstance | NodeKind::MacroCell => {
                    build_logic_expr(&mut exprs, &mut logic_exprs, node, operands, config)?
                }
                NodeKind::Dff => {
                    build_dffe_expr(&mut exprs, &mut logic_exprs, node, operands, config)?
                }
            };

            expr_of_node[node_index] = Some(expr_id);
        }

        let mut output_exprs = Vec::new();
        for node in original.nodes() {
            if outdegree[node.id.0] == 0 {
                output_exprs
                    .push(expr_of_node[node.id.0].ok_or(SynthError::CycleOrUnsupportedDependency)?);
            }
        }

        let live_exprs = mark_live_exprs(&exprs, &output_exprs);

        let mut rewritten = Netlist::new();
        let mut driver_by_expr = vec![None::<PinRef>; exprs.len()];

        for (expr_id, expr) in exprs.iter().enumerate() {
            if matches!(expr.kind, LogicExprKind::Input) {
                let input_node = rewritten.add_node(NodeKind::Port, expr.name.clone());
                driver_by_expr[expr_id] = Some(PinRef {
                    node: input_node,
                    port: 0,
                });
                continue;
            }
            if !live_exprs[expr_id] {
                continue;
            }
        }

        for (expr_id, expr) in exprs.iter().enumerate() {
            if !live_exprs[expr_id] {
                continue;
            }
            if matches!(expr.kind, LogicExprKind::Input) {
                continue;
            }

            let gate_node = rewritten.add_node_with_logic(
                expr.node_kind.clone(),
                expr.name.clone(),
                expr.logic_op.clone(),
            );
            for (port, input_expr) in expr_inputs(&expr.kind).iter().enumerate() {
                let driver =
                    driver_by_expr[*input_expr].ok_or(SynthError::CycleOrUnsupportedDependency)?;
                self.connect_with_splitter(
                    &mut rewritten,
                    driver,
                    PinRef {
                        node: gate_node,
                        port: port as u16,
                    },
                )?;
            }

            driver_by_expr[expr_id] = Some(PinRef {
                node: gate_node,
                port: 0,
            });
        }

        let mut terminal_uses = HashMap::<PinRef, usize>::new();
        for node in original.nodes() {
            if outdegree[node.id.0] != 0 {
                continue;
            }

            let driver = driver_by_expr
                [expr_of_node[node.id.0].ok_or(SynthError::CycleOrUnsupportedDependency)?]
            .ok_or(SynthError::CycleOrUnsupportedDependency)?;
            let seen = terminal_uses.get(&driver).copied().unwrap_or(0);
            let requires_explicit_output =
                matches!(node.kind, NodeKind::Port) && !incoming_by_node[node.id.0].is_empty();

            if requires_explicit_output || seen > 0 || rewritten.sink_of(driver).is_some() {
                let output_node = rewritten.add_node(NodeKind::Port, node.name.clone());
                self.connect_with_splitter(
                    &mut rewritten,
                    driver,
                    PinRef {
                        node: output_node,
                        port: 0,
                    },
                )?;
            }

            terminal_uses.insert(driver, seen + 1);
        }

        Ok(rewritten)
    }
}

fn incoming_and_outdegree(netlist: &Netlist) -> (Vec<Vec<(u16, PinRef)>>, Vec<usize>) {
    let mut incoming_by_node = vec![Vec::<(u16, PinRef)>::new(); netlist.node_count()];
    let mut outdegree = vec![0usize; netlist.node_count()];

    for (from, to) in netlist.edge_pairs() {
        incoming_by_node[to.node.0].push((to.port, from));
        outdegree[from.node.0] += 1;
    }

    (incoming_by_node, outdegree)
}

fn topological_order(netlist: &Netlist) -> Result<Vec<usize>, SynthError> {
    let mut indegree = vec![0usize; netlist.node_count()];
    let mut adjacency = vec![Vec::<usize>::new(); netlist.node_count()];
    for (from, to) in netlist.edge_pairs() {
        indegree[to.node.0] += 1;
        adjacency[from.node.0].push(to.node.0);
    }

    let mut queue = VecDeque::new();
    for (node, degree) in indegree.iter().enumerate() {
        if *degree == 0 {
            queue.push_back(node);
        }
    }

    let mut order = Vec::with_capacity(netlist.node_count());
    while let Some(node) = queue.pop_front() {
        order.push(node);
        for succ in &adjacency[node] {
            indegree[*succ] -= 1;
            if indegree[*succ] == 0 {
                queue.push_back(*succ);
            }
        }
    }

    if order.len() != netlist.node_count() {
        return Err(SynthError::CombinationalCycle);
    }

    Ok(order)
}

fn canonicalize_and_operands(
    exprs: &[LogicExpr],
    operands: Vec<usize>,
    flattening_limit: usize,
) -> Vec<usize> {
    let mut flattened = Vec::new();

    for operand in operands {
        match &exprs[operand].kind {
            LogicExprKind::And(children)
                if flattening_limit > 0 && flattened.len() + children.len() <= flattening_limit =>
            {
                flattened.extend(children.iter().copied());
            }
            _ => flattened.push(operand),
        }
    }

    flattened.sort_unstable();
    flattened.dedup();
    remove_absorbed_operands(exprs, flattened, LogicAbsorptionMode::And)
}

fn canonicalize_or_operands(
    exprs: &[LogicExpr],
    operands: Vec<usize>,
    flattening_limit: usize,
) -> Vec<usize> {
    let mut flattened = Vec::new();

    for operand in operands {
        match &exprs[operand].kind {
            LogicExprKind::Or(children)
                if flattening_limit > 0 && flattened.len() + children.len() <= flattening_limit =>
            {
                flattened.extend(children.iter().copied());
            }
            _ => flattened.push(operand),
        }
    }

    flattened.sort_unstable();
    flattened.dedup();
    remove_absorbed_operands(exprs, flattened, LogicAbsorptionMode::Or)
}

fn canonicalize_xor_operands(
    exprs: &[LogicExpr],
    operands: Vec<usize>,
    flattening_limit: usize,
) -> Vec<usize> {
    let mut flattened = Vec::new();

    for operand in operands {
        match &exprs[operand].kind {
            LogicExprKind::Xor(children)
                if flattening_limit > 0 && flattened.len() + children.len() <= flattening_limit =>
            {
                flattened.extend(children.iter().copied());
            }
            _ => flattened.push(operand),
        }
    }

    flattened.sort_unstable();
    flattened
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum LogicAbsorptionMode {
    And,
    Or,
}

fn remove_absorbed_operands(
    exprs: &[LogicExpr],
    operands: Vec<usize>,
    mode: LogicAbsorptionMode,
) -> Vec<usize> {
    if operands.len() < 2 {
        return operands;
    }

    let mut kept = Vec::with_capacity(operands.len());
    for operand in &operands {
        let absorbed = match mode {
            LogicAbsorptionMode::And => operand_absorbed_in_and(exprs, *operand, &operands),
            LogicAbsorptionMode::Or => operand_absorbed_in_or(exprs, *operand, &operands),
        };
        if !absorbed {
            kept.push(*operand);
        }
    }

    if kept.is_empty() {
        operands
    } else {
        kept
    }
}

fn operand_absorbed_in_and(exprs: &[LogicExpr], candidate: usize, operands: &[usize]) -> bool {
    if let LogicExprKind::Or(children) = &exprs[candidate].kind {
        return operands
            .iter()
            .copied()
            .any(|other| other != candidate && operand_is_or_subset(exprs, other, children))
            || operand_absorbed_by_and_consensus(exprs, candidate, operands);
    }
    false
}

fn operand_absorbed_in_or(exprs: &[LogicExpr], candidate: usize, operands: &[usize]) -> bool {
    if let LogicExprKind::And(children) = &exprs[candidate].kind {
        return operands
            .iter()
            .copied()
            .any(|other| other != candidate && operand_is_and_subset(exprs, other, children))
            || operand_absorbed_by_or_consensus(exprs, candidate, operands);
    }
    false
}

fn operand_is_or_subset(exprs: &[LogicExpr], operand: usize, superset_children: &[usize]) -> bool {
    if superset_children.binary_search(&operand).is_ok() {
        return true;
    }

    match &exprs[operand].kind {
        LogicExprKind::Or(children) => children
            .iter()
            .all(|child| superset_children.binary_search(child).is_ok()),
        _ => false,
    }
}

fn operand_is_and_subset(exprs: &[LogicExpr], operand: usize, superset_children: &[usize]) -> bool {
    if superset_children.binary_search(&operand).is_ok() {
        return true;
    }

    match &exprs[operand].kind {
        LogicExprKind::And(children) => children
            .iter()
            .all(|child| superset_children.binary_search(child).is_ok()),
        _ => false,
    }
}

fn operand_absorbed_by_or_consensus(
    exprs: &[LogicExpr],
    candidate: usize,
    operands: &[usize],
) -> bool {
    let LogicExprKind::And(candidate_children) = &exprs[candidate].kind else {
        return false;
    };
    let candidate_literals = normalized_child_literals(exprs, candidate_children);

    for (index, left) in operands.iter().copied().enumerate() {
        if left == candidate {
            continue;
        }
        for right in operands.iter().copied().skip(index + 1) {
            if right == candidate {
                continue;
            }
            if consensus_union_matches(exprs, left, right, &candidate_literals, true) {
                return true;
            }
        }
    }

    false
}

fn operand_absorbed_by_and_consensus(
    exprs: &[LogicExpr],
    candidate: usize,
    operands: &[usize],
) -> bool {
    let LogicExprKind::Or(candidate_children) = &exprs[candidate].kind else {
        return false;
    };
    let candidate_literals = normalized_child_literals(exprs, candidate_children);

    for (index, left) in operands.iter().copied().enumerate() {
        if left == candidate {
            continue;
        }
        for right in operands.iter().copied().skip(index + 1) {
            if right == candidate {
                continue;
            }
            if consensus_union_matches(exprs, left, right, &candidate_literals, false) {
                return true;
            }
        }
    }

    false
}

fn consensus_union_matches(
    exprs: &[LogicExpr],
    left: usize,
    right: usize,
    candidate_literals: &[(usize, bool)],
    require_and_terms: bool,
) -> bool {
    let (left_children, right_children) =
        match (&exprs[left].kind, &exprs[right].kind, require_and_terms) {
            (LogicExprKind::And(left_children), LogicExprKind::And(right_children), true) => {
                (left_children.as_slice(), right_children.as_slice())
            }
            (LogicExprKind::Or(left_children), LogicExprKind::Or(right_children), false) => {
                (left_children.as_slice(), right_children.as_slice())
            }
            _ => return false,
        };

    let left_literals = normalized_child_literals(exprs, left_children);
    let right_literals = normalized_child_literals(exprs, right_children);

    for (left_index, left_literal) in left_literals.iter().enumerate() {
        for (right_index, right_literal) in right_literals.iter().enumerate() {
            if left_literal.0 != right_literal.0 || left_literal.1 == right_literal.1 {
                continue;
            }

            let mut union = Vec::new();
            union.extend(
                left_literals
                    .iter()
                    .enumerate()
                    .filter_map(|(index, literal)| (index != left_index).then_some(*literal)),
            );
            union.extend(
                right_literals
                    .iter()
                    .enumerate()
                    .filter_map(|(index, literal)| (index != right_index).then_some(*literal)),
            );
            union.sort_unstable();
            union.dedup();

            if union == candidate_literals {
                return true;
            }
        }
    }

    false
}

fn normalized_child_literals(exprs: &[LogicExpr], children: &[usize]) -> Vec<(usize, bool)> {
    let mut literals: Vec<(usize, bool)> = children
        .iter()
        .copied()
        .map(|expr| expr_literal(exprs, expr))
        .collect();
    literals.sort_unstable();
    literals.dedup();
    literals
}

fn expect_exact_inputs(
    report: &mut BoolOptCompatibilityReport,
    node: usize,
    actual: usize,
    expected: usize,
) {
    if actual != expected {
        report.issues.push(BoolOptCompatibilityIssue {
            node,
            kind: BoolOptCompatibilityIssueKind::UnexpectedInputCount,
            detail: format!("expected {expected} input(s), found {actual}"),
        });
    }
}

fn expected_logic_inputs(node: &rflux_ir::Node) -> Option<usize> {
    match node.logic_op.clone().unwrap_or(LogicOp::And) {
        LogicOp::Buf => Some(1),
        LogicOp::Not => Some(1),
        LogicOp::Mux2 => Some(3),
        LogicOp::And | LogicOp::Or | LogicOp::Xor => None,
        LogicOp::DffEnable => Some(3),
    }
}

fn ensure_input_count(node: usize, actual: usize, expected: usize) -> Result<(), SynthError> {
    if actual == expected {
        return Ok(());
    }

    Err(SynthError::UnexpectedBoolOptInputCount {
        node,
        expected,
        actual,
    })
}

fn build_logic_expr(
    exprs: &mut Vec<LogicExpr>,
    logic_exprs: &mut BTreeMap<LogicExprKey, usize>,
    node: &rflux_ir::Node,
    operands: Vec<usize>,
    config: &BoolOptConfig,
) -> Result<usize, SynthError> {
    let logic_op = node.logic_op.clone().unwrap_or(LogicOp::And);
    match logic_op {
        LogicOp::Buf => {
            ensure_input_count(node.id.0, operands.len(), 1)?;
            Ok(operands[0])
        }
        LogicOp::Not => {
            ensure_input_count(node.id.0, operands.len(), 1)?;
            build_keyed_expr(
                exprs,
                logic_exprs,
                node,
                LogicExprKey::Not(operands[0]),
                LogicExprKind::Not(operands[0]),
                true,
            )
        }
        LogicOp::And => build_and_logic_expr(exprs, logic_exprs, node, operands, config),
        LogicOp::Or => build_or_logic_expr(exprs, logic_exprs, node, operands, config),
        LogicOp::Xor => {
            let use_sharing = config.infer_xor_mux;
            build_commutative_expr_with_toggle(
                exprs,
                logic_exprs,
                node,
                LogicExprKey::Xor,
                LogicExprKind::Xor,
                operands,
                config.share_logic_flattening_limit,
                canonicalize_xor_operands,
                use_sharing,
            )
        }
        LogicOp::Mux2 => {
            ensure_input_count(node.id.0, operands.len(), 3)?;
            let key = LogicExprKey::Mux2([operands[0], operands[1], operands[2]]);
            build_keyed_expr(
                exprs,
                logic_exprs,
                node,
                key,
                LogicExprKind::Mux2([operands[0], operands[1], operands[2]]),
                config.infer_xor_mux,
            )
        }
        LogicOp::DffEnable => Err(SynthError::UnsupportedBoolOptNodeKind(node.kind.clone())),
    }
}

fn build_and_logic_expr(
    exprs: &mut Vec<LogicExpr>,
    logic_exprs: &mut BTreeMap<LogicExprKey, usize>,
    node: &rflux_ir::Node,
    operands: Vec<usize>,
    config: &BoolOptConfig,
) -> Result<usize, SynthError> {
    let operands = canonicalize_and_operands(exprs, operands, config.share_logic_flattening_limit);
    if let Some(rewritten) =
        try_infer_xor_or_mux_from_or_pattern(exprs, logic_exprs, node, &operands, config)?
    {
        Ok(rewritten)
    } else if let Some(rewritten) =
        try_factor_and_of_or_common_term(exprs, logic_exprs, node, &operands, config)?
    {
        Ok(rewritten)
    } else {
        build_commutative_expr(
            exprs,
            logic_exprs,
            node,
            LogicExprKey::And,
            LogicExprKind::And,
            operands,
            config.share_logic_flattening_limit,
            canonicalize_and_operands,
        )
    }
}

fn build_or_logic_expr(
    exprs: &mut Vec<LogicExpr>,
    logic_exprs: &mut BTreeMap<LogicExprKey, usize>,
    node: &rflux_ir::Node,
    operands: Vec<usize>,
    config: &BoolOptConfig,
) -> Result<usize, SynthError> {
    let operands = canonicalize_or_operands(exprs, operands, config.share_logic_flattening_limit);
    if let Some(rewritten) =
        try_infer_xor_or_mux_from_and_pattern(exprs, logic_exprs, node, &operands, config)?
    {
        Ok(rewritten)
    } else if let Some(rewritten) =
        try_factor_or_of_and_common_term(exprs, logic_exprs, node, &operands, config)?
    {
        Ok(rewritten)
    } else {
        build_commutative_expr(
            exprs,
            logic_exprs,
            node,
            LogicExprKey::Or,
            LogicExprKind::Or,
            operands,
            config.share_logic_flattening_limit,
            canonicalize_or_operands,
        )
    }
}

fn try_infer_xor_or_mux_from_and_pattern(
    exprs: &mut Vec<LogicExpr>,
    logic_exprs: &mut BTreeMap<LogicExprKey, usize>,
    node: &rflux_ir::Node,
    operands: &[usize],
    config: &BoolOptConfig,
) -> Result<Option<usize>, SynthError> {
    if !config.infer_xor_mux || operands.len() != 2 {
        return Ok(None);
    }

    let Some(left_terms) = binary_and_literals(exprs, operands[0]) else {
        return Ok(None);
    };
    let Some(right_terms) = binary_and_literals(exprs, operands[1]) else {
        return Ok(None);
    };

    if let Some([lhs, rhs]) = try_match_xor_pattern(exprs, left_terms, right_terms) {
        let mut xor_node = node.clone();
        xor_node.logic_op = Some(LogicOp::Xor);
        return build_commutative_expr_with_toggle(
            exprs,
            logic_exprs,
            &xor_node,
            LogicExprKey::Xor,
            LogicExprKind::Xor,
            vec![lhs, rhs],
            config.share_logic_flattening_limit,
            canonicalize_xor_operands,
            true,
        )
        .map(Some);
    }

    if let Some([sel, when_true, when_false]) =
        try_match_mux_pattern(exprs, left_terms, right_terms)
    {
        let mut mux_node = node.clone();
        mux_node.logic_op = Some(LogicOp::Mux2);
        return build_keyed_expr(
            exprs,
            logic_exprs,
            &mux_node,
            LogicExprKey::Mux2([sel, when_true, when_false]),
            LogicExprKind::Mux2([sel, when_true, when_false]),
            true,
        )
        .map(Some);
    }

    Ok(None)
}

fn try_infer_xor_or_mux_from_or_pattern(
    exprs: &mut Vec<LogicExpr>,
    logic_exprs: &mut BTreeMap<LogicExprKey, usize>,
    node: &rflux_ir::Node,
    operands: &[usize],
    config: &BoolOptConfig,
) -> Result<Option<usize>, SynthError> {
    if !config.infer_xor_mux || operands.len() != 2 {
        return Ok(None);
    }

    let Some(left_terms) = binary_or_literals(exprs, operands[0]) else {
        return Ok(None);
    };
    let Some(right_terms) = binary_or_literals(exprs, operands[1]) else {
        return Ok(None);
    };

    if let Some([lhs, rhs]) = try_match_dual_xor_pattern(exprs, left_terms, right_terms) {
        let mut xor_node = node.clone();
        xor_node.logic_op = Some(LogicOp::Xor);
        return build_commutative_expr_with_toggle(
            exprs,
            logic_exprs,
            &xor_node,
            LogicExprKey::Xor,
            LogicExprKind::Xor,
            vec![lhs, rhs],
            config.share_logic_flattening_limit,
            canonicalize_xor_operands,
            true,
        )
        .map(Some);
    }

    if let Some([sel, when_true, when_false]) =
        try_match_dual_mux_pattern(exprs, left_terms, right_terms)
    {
        let mut mux_node = node.clone();
        mux_node.logic_op = Some(LogicOp::Mux2);
        return build_keyed_expr(
            exprs,
            logic_exprs,
            &mux_node,
            LogicExprKey::Mux2([sel, when_true, when_false]),
            LogicExprKind::Mux2([sel, when_true, when_false]),
            true,
        )
        .map(Some);
    }

    Ok(None)
}

fn binary_and_literals(exprs: &[LogicExpr], expr: usize) -> Option<[usize; 2]> {
    match &exprs[expr].kind {
        LogicExprKind::And(children) if children.len() == 2 => Some([children[0], children[1]]),
        _ => None,
    }
}

fn binary_or_literals(exprs: &[LogicExpr], expr: usize) -> Option<[usize; 2]> {
    match &exprs[expr].kind {
        LogicExprKind::Or(children) if children.len() == 2 => Some([children[0], children[1]]),
        _ => None,
    }
}

fn expr_literal(exprs: &[LogicExpr], expr: usize) -> (usize, bool) {
    match exprs[expr].kind {
        LogicExprKind::Not(inner) => (inner, true),
        _ => (expr, false),
    }
}

fn try_match_xor_pattern(
    exprs: &[LogicExpr],
    left_terms: [usize; 2],
    right_terms: [usize; 2],
) -> Option<[usize; 2]> {
    let left = [
        expr_literal(exprs, left_terms[0]),
        expr_literal(exprs, left_terms[1]),
    ];
    let right = [
        expr_literal(exprs, right_terms[0]),
        expr_literal(exprs, right_terms[1]),
    ];

    for right_order in [[right[0], right[1]], [right[1], right[0]]] {
        if left[0].0 == right_order[0].0
            && left[1].0 == right_order[1].0
            && left[0].0 != left[1].0
            && left[0].1 != right_order[0].1
            && left[1].1 != right_order[1].1
            && left[0].1 != left[1].1
        {
            let mut inputs = [left[0].0, left[1].0];
            inputs.sort_unstable();
            return Some(inputs);
        }
    }

    None
}

fn try_match_mux_pattern(
    exprs: &[LogicExpr],
    left_terms: [usize; 2],
    right_terms: [usize; 2],
) -> Option<[usize; 3]> {
    for &left_sel_index in &[0usize, 1] {
        for &right_sel_index in &[0usize, 1] {
            let left_sel = expr_literal(exprs, left_terms[left_sel_index]);
            let right_sel = expr_literal(exprs, right_terms[right_sel_index]);
            if left_sel.0 != right_sel.0 || left_sel.1 == right_sel.1 {
                continue;
            }

            let left_data = left_terms[1 - left_sel_index];
            let right_data = right_terms[1 - right_sel_index];
            let (when_true, when_false) = if !left_sel.1 {
                (left_data, right_data)
            } else {
                (right_data, left_data)
            };

            return Some([left_sel.0, when_true, when_false]);
        }
    }

    None
}

fn try_match_dual_xor_pattern(
    exprs: &[LogicExpr],
    left_terms: [usize; 2],
    right_terms: [usize; 2],
) -> Option<[usize; 2]> {
    let left = [
        expr_literal(exprs, left_terms[0]),
        expr_literal(exprs, left_terms[1]),
    ];
    let right = [
        expr_literal(exprs, right_terms[0]),
        expr_literal(exprs, right_terms[1]),
    ];

    for right_order in [[right[0], right[1]], [right[1], right[0]]] {
        if left[0].0 == right_order[0].0
            && left[1].0 == right_order[1].0
            && left[0].0 != left[1].0
            && left[0].1 != right_order[0].1
            && left[1].1 != right_order[1].1
            && left[0].1 == left[1].1
        {
            let mut inputs = [left[0].0, left[1].0];
            inputs.sort_unstable();
            return Some(inputs);
        }
    }

    None
}

fn try_match_dual_mux_pattern(
    exprs: &[LogicExpr],
    left_terms: [usize; 2],
    right_terms: [usize; 2],
) -> Option<[usize; 3]> {
    for &left_sel_index in &[0usize, 1] {
        for &right_sel_index in &[0usize, 1] {
            let left_sel = expr_literal(exprs, left_terms[left_sel_index]);
            let right_sel = expr_literal(exprs, right_terms[right_sel_index]);
            if left_sel.0 != right_sel.0 || left_sel.1 == right_sel.1 {
                continue;
            }

            let left_data = left_terms[1 - left_sel_index];
            let right_data = right_terms[1 - right_sel_index];
            let (when_true, when_false) = if !left_sel.1 {
                (right_data, left_data)
            } else {
                (left_data, right_data)
            };

            return Some([left_sel.0, when_true, when_false]);
        }
    }

    None
}

fn try_factor_or_of_and_common_term(
    exprs: &mut Vec<LogicExpr>,
    logic_exprs: &mut BTreeMap<LogicExprKey, usize>,
    node: &rflux_ir::Node,
    operands: &[usize],
    config: &BoolOptConfig,
) -> Result<Option<usize>, SynthError> {
    if operands.len() < 2 {
        return Ok(None);
    }

    let mut and_terms = Vec::with_capacity(operands.len());
    for operand in operands {
        match &exprs[*operand].kind {
            LogicExprKind::And(children) => and_terms.push(children.clone()),
            _ => return Ok(None),
        }
    }

    let mut common_terms = and_terms[0].to_vec();
    common_terms.retain(|signal| {
        and_terms[1..]
            .iter()
            .all(|children| children.binary_search(signal).is_ok())
    });

    if common_terms.is_empty() {
        return Ok(None);
    }

    let mut and_node = node.clone();
    and_node.logic_op = Some(LogicOp::And);
    let mut or_node = node.clone();
    or_node.logic_op = Some(LogicOp::Or);

    let mut factored_terms = Vec::with_capacity(and_terms.len());
    for children in and_terms {
        let tail: Vec<usize> = children
            .iter()
            .copied()
            .filter(|signal| common_terms.binary_search(signal).is_err())
            .collect();
        if tail.is_empty() {
            return Ok(None);
        }
        factored_terms.push(build_and_tail(exprs, logic_exprs, &and_node, tail, config)?);
    }

    let or_expr = build_or_logic_expr(exprs, logic_exprs, &or_node, factored_terms, config)?;

    common_terms.push(or_expr);

    let and_expr = build_and_logic_expr(exprs, logic_exprs, &and_node, common_terms, config)?;

    Ok(Some(and_expr))
}

fn try_factor_and_of_or_common_term(
    exprs: &mut Vec<LogicExpr>,
    logic_exprs: &mut BTreeMap<LogicExprKey, usize>,
    node: &rflux_ir::Node,
    operands: &[usize],
    config: &BoolOptConfig,
) -> Result<Option<usize>, SynthError> {
    if operands.len() < 2 {
        return Ok(None);
    }

    let mut or_terms = Vec::with_capacity(operands.len());
    for operand in operands {
        match &exprs[*operand].kind {
            LogicExprKind::Or(children) => or_terms.push(children.clone()),
            _ => return Ok(None),
        }
    }

    let mut common_terms = or_terms[0].clone();
    common_terms.retain(|signal| {
        or_terms[1..]
            .iter()
            .all(|children| children.binary_search(signal).is_ok())
    });

    if common_terms.is_empty() {
        return Ok(None);
    }

    let mut and_node = node.clone();
    and_node.logic_op = Some(LogicOp::And);
    let mut or_node = node.clone();
    or_node.logic_op = Some(LogicOp::Or);

    let mut factored_terms = Vec::with_capacity(or_terms.len());
    for children in or_terms {
        let tail: Vec<usize> = children
            .iter()
            .copied()
            .filter(|signal| common_terms.binary_search(signal).is_err())
            .collect();
        if tail.is_empty() {
            return Ok(None);
        }
        factored_terms.push(build_or_tail(exprs, logic_exprs, &or_node, tail, config)?);
    }

    let and_expr = build_and_logic_expr(exprs, logic_exprs, &and_node, factored_terms, config)?;

    common_terms.push(and_expr);

    let or_expr = build_or_logic_expr(exprs, logic_exprs, &or_node, common_terms, config)?;

    Ok(Some(or_expr))
}

fn build_and_tail(
    exprs: &mut Vec<LogicExpr>,
    logic_exprs: &mut BTreeMap<LogicExprKey, usize>,
    node: &rflux_ir::Node,
    operands: Vec<usize>,
    config: &BoolOptConfig,
) -> Result<usize, SynthError> {
    if operands.len() == 1 {
        return Ok(operands[0]);
    }

    build_and_logic_expr(exprs, logic_exprs, node, operands, config)
}

fn build_or_tail(
    exprs: &mut Vec<LogicExpr>,
    logic_exprs: &mut BTreeMap<LogicExprKey, usize>,
    node: &rflux_ir::Node,
    operands: Vec<usize>,
    config: &BoolOptConfig,
) -> Result<usize, SynthError> {
    if operands.len() == 1 {
        return Ok(operands[0]);
    }

    build_or_logic_expr(exprs, logic_exprs, node, operands, config)
}

fn build_dffe_expr(
    exprs: &mut Vec<LogicExpr>,
    logic_exprs: &mut BTreeMap<LogicExprKey, usize>,
    node: &rflux_ir::Node,
    operands: Vec<usize>,
    config: &BoolOptConfig,
) -> Result<usize, SynthError> {
    match node.logic_op {
        Some(LogicOp::DffEnable) => {
            ensure_input_count(node.id.0, operands.len(), 3)?;
            let key = LogicExprKey::DffEnable([operands[0], operands[1], operands[2]]);
            build_keyed_expr(
                exprs,
                logic_exprs,
                node,
                key,
                LogicExprKind::DffEnable([operands[0], operands[1], operands[2]]),
                config.infer_dffe,
            )
        }
        _ => Err(SynthError::UnsupportedBoolOptNodeKind(NodeKind::Dff)),
    }
}

fn build_commutative_expr<FCanonicalize, FKind, FKey>(
    exprs: &mut Vec<LogicExpr>,
    logic_exprs: &mut BTreeMap<LogicExprKey, usize>,
    node: &rflux_ir::Node,
    key_ctor: FKey,
    kind_ctor: FKind,
    operands: Vec<usize>,
    flattening_limit: usize,
    canonicalize: FCanonicalize,
) -> Result<usize, SynthError>
where
    FCanonicalize: Fn(&[LogicExpr], Vec<usize>, usize) -> Vec<usize>,
    FKind: Fn(Vec<usize>) -> LogicExprKind,
    FKey: Fn(Vec<usize>) -> LogicExprKey,
{
    build_commutative_expr_with_toggle(
        exprs,
        logic_exprs,
        node,
        key_ctor,
        kind_ctor,
        operands,
        flattening_limit,
        canonicalize,
        true,
    )
}

fn build_commutative_expr_with_toggle<FCanonicalize, FKind, FKey>(
    exprs: &mut Vec<LogicExpr>,
    logic_exprs: &mut BTreeMap<LogicExprKey, usize>,
    node: &rflux_ir::Node,
    key_ctor: FKey,
    kind_ctor: FKind,
    operands: Vec<usize>,
    flattening_limit: usize,
    canonicalize: FCanonicalize,
    enable_sharing: bool,
) -> Result<usize, SynthError>
where
    FCanonicalize: Fn(&[LogicExpr], Vec<usize>, usize) -> Vec<usize>,
    FKind: Fn(Vec<usize>) -> LogicExprKind,
    FKey: Fn(Vec<usize>) -> LogicExprKey,
{
    let operands = canonicalize(exprs, operands, flattening_limit);
    if operands.is_empty() {
        return Err(SynthError::MissingBoolOptDriver(node.id.0));
    }
    if operands.len() == 1 {
        return Ok(operands[0]);
    }

    let key = key_ctor(operands.clone());
    build_keyed_expr(
        exprs,
        logic_exprs,
        node,
        key,
        kind_ctor(operands),
        enable_sharing,
    )
}

fn build_keyed_expr(
    exprs: &mut Vec<LogicExpr>,
    logic_exprs: &mut BTreeMap<LogicExprKey, usize>,
    node: &rflux_ir::Node,
    key: LogicExprKey,
    kind: LogicExprKind,
    enable_sharing: bool,
) -> Result<usize, SynthError> {
    if enable_sharing {
        if let Some(existing) = logic_exprs.get(&key) {
            return Ok(*existing);
        }
    }

    let expr_id = exprs.len();
    exprs.push(LogicExpr {
        kind,
        node_kind: node.kind.clone(),
        name: node.name.clone(),
        logic_op: node.logic_op.clone(),
    });
    if enable_sharing {
        logic_exprs.insert(key, expr_id);
    }
    Ok(expr_id)
}

fn expr_inputs(kind: &LogicExprKind) -> Vec<usize> {
    match kind {
        LogicExprKind::Input => Vec::new(),
        LogicExprKind::Not(input) => vec![*input],
        LogicExprKind::And(inputs) | LogicExprKind::Or(inputs) | LogicExprKind::Xor(inputs) => {
            inputs.clone()
        }
        LogicExprKind::Mux2(inputs) | LogicExprKind::DffEnable(inputs) => inputs.to_vec(),
    }
}

fn count_logic_gates(netlist: &Netlist) -> usize {
    netlist
        .nodes()
        .iter()
        .filter(|node| matches!(node.kind, NodeKind::CellInstance | NodeKind::MacroCell))
        .count()
}

fn mark_live_exprs(exprs: &[LogicExpr], roots: &[usize]) -> Vec<bool> {
    let mut live = vec![false; exprs.len()];
    let mut stack = roots.to_vec();

    while let Some(expr) = stack.pop() {
        if live[expr] {
            continue;
        }
        live[expr] = true;
        stack.extend(expr_inputs(&exprs[expr].kind));
    }

    live
}

fn normalize_mux_feedback_dffe(netlist: &Netlist) -> Result<Option<Netlist>, SynthError> {
    let (mut incoming_by_node, _) = incoming_and_outdegree(netlist);
    for incoming in &mut incoming_by_node {
        incoming.sort_by_key(|(port, _)| *port);
    }

    let mut rewritten = netlist.clone();
    let mut changed = false;

    for node in netlist.nodes() {
        if !matches!(node.kind, NodeKind::Dff) || node.logic_op.is_some() {
            continue;
        }

        let incoming = &incoming_by_node[node.id.0];
        if incoming.len() != 2 || incoming[0].0 != 0 {
            continue;
        }

        let data_source = incoming[0].1;
        let clock_source = incoming[1].1;
        let mux_source = trace_passthrough_source(netlist, data_source, &incoming_by_node);
        let mux_node = &netlist.nodes()[mux_source.node.0];
        if !matches!(mux_node.kind, NodeKind::CellInstance | NodeKind::MacroCell)
            || mux_node.logic_op != Some(LogicOp::Mux2)
        {
            continue;
        }

        let mux_inputs = &incoming_by_node[mux_node.id.0];
        if mux_inputs.len() != 3
            || mux_inputs[0].0 != 0
            || mux_inputs[1].0 != 1
            || mux_inputs[2].0 != 2
        {
            continue;
        }

        let select_source = mux_inputs[0].1;
        let arm_a_source = mux_inputs[1].1;
        let arm_b_source = mux_inputs[2].1;

        let (new_data_source, new_enable_source, needs_inverted_enable) =
            if trace_passthrough_source(netlist, arm_a_source, &incoming_by_node).node == node.id {
                (arm_b_source, select_source, false)
            } else if trace_passthrough_source(netlist, arm_b_source, &incoming_by_node).node
                == node.id
            {
                (arm_a_source, select_source, true)
            } else {
                continue;
            };

        disconnect_inputs(&mut rewritten, mux_node.id, &incoming_by_node);
        rewritten.disconnect(data_source);
        rewritten.disconnect(clock_source);

        let enable_source = if needs_inverted_enable {
            let inverted_enable = rewritten.add_node_with_logic(
                NodeKind::CellInstance,
                format!("{}_auto_enable_inv", node.name),
                Some(LogicOp::Not),
            );
            rewritten
                .connect(
                    select_source,
                    PinRef {
                        node: inverted_enable,
                        port: 0,
                    },
                )
                .map_err(SynthError::from)?;
            PinRef {
                node: inverted_enable,
                port: 0,
            }
        } else {
            new_enable_source
        };

        rewritten
            .connect(
                new_data_source,
                PinRef {
                    node: node.id,
                    port: 0,
                },
            )
            .map_err(SynthError::from)?;
        rewritten
            .connect(
                enable_source,
                PinRef {
                    node: node.id,
                    port: 1,
                },
            )
            .map_err(SynthError::from)?;
        rewritten
            .connect(
                clock_source,
                PinRef {
                    node: node.id,
                    port: 2,
                },
            )
            .map_err(SynthError::from)?;

        let rewritten_node = rewritten.node_mut(node.id).ok_or_else(|| {
            SynthError::SatEncoding(format!("missing rewritten Dff node {}", node.id.0))
        })?;
        rewritten_node.logic_op = Some(LogicOp::DffEnable);
        changed = true;
    }

    if !changed {
        return Ok(None);
    }

    Ok(Some(prune_to_observable_nodes(&rewritten)?))
}

fn disconnect_inputs(netlist: &mut Netlist, node: NodeId, incoming_by_node: &[Vec<(u16, PinRef)>]) {
    for (_, source) in &incoming_by_node[node.0] {
        netlist.disconnect(*source);
    }
}

fn trace_passthrough_source(
    netlist: &Netlist,
    mut source: PinRef,
    incoming_by_node: &[Vec<(u16, PinRef)>],
) -> PinRef {
    loop {
        let node = &netlist.nodes()[source.node.0];
        if !matches!(
            node.kind,
            NodeKind::Port | NodeKind::Splitter | NodeKind::Jtl | NodeKind::Ptl
        ) {
            return source;
        }

        let incoming = &incoming_by_node[source.node.0];
        if incoming.len() != 1 {
            return source;
        }

        source = incoming[0].1;
    }
}

fn prune_to_observable_nodes(netlist: &Netlist) -> Result<Netlist, SynthError> {
    let (incoming_by_node, outdegree) = incoming_and_outdegree(netlist);
    let mut keep = vec![false; netlist.node_count()];
    let mut worklist = VecDeque::new();

    for node in netlist.nodes() {
        let is_observable_port = matches!(node.kind, NodeKind::Port)
            && !incoming_by_node[node.id.0].is_empty()
            && outdegree[node.id.0] == 0;
        if is_observable_port || matches!(node.kind, NodeKind::Dff) {
            keep[node.id.0] = true;
            worklist.push_back(node.id.0);
        }
    }

    while let Some(node_index) = worklist.pop_front() {
        for (_, source) in &incoming_by_node[node_index] {
            if !keep[source.node.0] {
                keep[source.node.0] = true;
                worklist.push_back(source.node.0);
            }
        }
    }

    let mut pruned = Netlist::new();
    let mut remap = vec![None; netlist.node_count()];
    for node in netlist.nodes() {
        if !keep[node.id.0] {
            continue;
        }

        let new_id =
            pruned.add_node_with_logic(node.kind.clone(), node.name.clone(), node.logic_op.clone());
        remap[node.id.0] = Some(new_id);
    }

    for (from, to) in netlist.edge_pairs() {
        let Some(mapped_from) = remap[from.node.0] else {
            continue;
        };
        let Some(mapped_to) = remap[to.node.0] else {
            continue;
        };

        pruned
            .connect(
                PinRef {
                    node: mapped_from,
                    port: from.port,
                },
                PinRef {
                    node: mapped_to,
                    port: to.port,
                },
            )
            .map_err(SynthError::from)?;
    }

    Ok(pruned)
}
