use std::collections::{BTreeMap, HashMap, VecDeque};

use rflux_ir::{LogicOp, Netlist, NodeKind, PinRef};

use crate::{
    BoolOptCompatibilityIssue, BoolOptCompatibilityIssueKind, BoolOptCompatibilityReport,
    BoolOptConfig, BoolOptReport, Compiler, SynthError,
};

#[derive(Debug, Clone)]
enum LogicExprKind {
    Input,
    And(Vec<usize>),
    Or(Vec<usize>),
    Xor(Vec<usize>),
    Mux2([usize; 3]),
    DffEnable([usize; 3]),
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
enum LogicExprKey {
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
        let compatibility = self.analyze_bool_opt_compatibility(netlist);
        let gate_count_before = count_logic_gates(netlist);

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
                    expect_exact_inputs(&mut report, node.id.0, incoming_by_node[node.id.0].len(), 1);
                }
                NodeKind::Dff => match node.logic_op {
                    Some(LogicOp::DffEnable) => {
                        expect_exact_inputs(&mut report, node.id.0, incoming_by_node[node.id.0].len(), 3);
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
            if !resolved[node.id.0]
                && !report.issues.iter().any(|issue| issue.node == node.id.0)
            {
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
                    expr_of_node[source.node.0]
                        .ok_or(SynthError::CycleOrUnsupportedDependency)?,
                );
            }

            let expr_id = match node.kind {
                NodeKind::Port | NodeKind::Splitter | NodeKind::Jtl | NodeKind::Ptl => {
                    ensure_input_count(node_index, operands.len(), 1)?;
                    operands[0]
                }
                NodeKind::CellInstance | NodeKind::MacroCell => build_logic_expr(
                    &mut exprs,
                    &mut logic_exprs,
                    node,
                    operands,
                    config,
                )?,
                NodeKind::Dff => build_dffe_expr(
                    &mut exprs,
                    &mut logic_exprs,
                    node,
                    operands,
                    config,
                )?,
            };

            expr_of_node[node_index] = Some(expr_id);
        }

        let mut output_exprs = Vec::new();
        for node in original.nodes() {
            if outdegree[node.id.0] == 0 {
                output_exprs.push(
                    expr_of_node[node.id.0].ok_or(SynthError::CycleOrUnsupportedDependency)?,
                );
            }
        }

        let live_exprs = mark_live_exprs(&exprs, &output_exprs);

        let mut rewritten = Netlist::new();
        let mut driver_by_expr = vec![None::<PinRef>; exprs.len()];

        for (expr_id, expr) in exprs.iter().enumerate() {
            if !live_exprs[expr_id] {
                continue;
            }
            if matches!(expr.kind, LogicExprKind::Input) {
                let input_node = rewritten.add_node(NodeKind::Port, expr.name.clone());
                driver_by_expr[expr_id] = Some(PinRef {
                    node: input_node,
                    port: 0,
                });
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
                let driver = driver_by_expr[*input_expr].ok_or(SynthError::CycleOrUnsupportedDependency)?;
                self.connect_with_splitter(
                    &mut rewritten,
                    driver,
                    PinRef {
                        node: gate_node,
                        port: port as u16,
                    },
                )?;
            }

            driver_by_expr[expr_id] = Some(PinRef { node: gate_node, port: 0 });
        }

        let mut terminal_uses = HashMap::<PinRef, usize>::new();
        for node in original.nodes() {
            if outdegree[node.id.0] != 0 {
                continue;
            }

            let driver = driver_by_expr[expr_of_node[node.id.0]
                .ok_or(SynthError::CycleOrUnsupportedDependency)?]
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
        return children.iter().any(|child| operands.binary_search(child).is_ok());
    }
    false
}

fn operand_absorbed_in_or(exprs: &[LogicExpr], candidate: usize, operands: &[usize]) -> bool {
    if let LogicExprKind::And(children) = &exprs[candidate].kind {
        return children.iter().any(|child| operands.binary_search(child).is_ok());
    }
    false
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
        LogicOp::And => build_commutative_expr(
            exprs,
            logic_exprs,
            node,
            LogicExprKey::And,
            |inputs| LogicExprKind::And(inputs),
            operands,
            config.share_logic_flattening_limit,
            canonicalize_and_operands,
        ),
        LogicOp::Or => {
            if let Some(rewritten) = try_factor_or_of_and_common_term(
                exprs,
                logic_exprs,
                node,
                &operands,
                config,
            )? {
                Ok(rewritten)
            } else {
                build_commutative_expr(
                    exprs,
                    logic_exprs,
                    node,
                    LogicExprKey::Or,
                    |inputs| LogicExprKind::Or(inputs),
                    operands,
                    config.share_logic_flattening_limit,
                    canonicalize_or_operands,
                )
            }
        }
        LogicOp::Xor => {
            let use_sharing = config.infer_xor_mux;
            build_commutative_expr_with_toggle(
                exprs,
                logic_exprs,
                node,
                LogicExprKey::Xor,
                |inputs| LogicExprKind::Xor(inputs),
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

fn try_factor_or_of_and_common_term(
    exprs: &mut Vec<LogicExpr>,
    logic_exprs: &mut BTreeMap<LogicExprKey, usize>,
    node: &rflux_ir::Node,
    operands: &[usize],
    config: &BoolOptConfig,
) -> Result<Option<usize>, SynthError> {
    if operands.len() != 2 {
        return Ok(None);
    }

    let left = match &exprs[operands[0]].kind {
        LogicExprKind::And(children) => children,
        _ => return Ok(None),
    };
    let right = match &exprs[operands[1]].kind {
        LogicExprKind::And(children) => children,
        _ => return Ok(None),
    };

    let Some(common) = left.iter().copied().find(|signal| right.binary_search(signal).is_ok()) else {
        return Ok(None);
    };

    let left_rest: Vec<usize> = left.iter().copied().filter(|s| *s != common).collect();
    let right_rest: Vec<usize> = right.iter().copied().filter(|s| *s != common).collect();
    if left_rest.is_empty() || right_rest.is_empty() {
        return Ok(None);
    }

    let mut and_node = node.clone();
    and_node.logic_op = Some(LogicOp::And);
    let mut or_node = node.clone();
    or_node.logic_op = Some(LogicOp::Or);

    let left_tail = build_and_tail(exprs, logic_exprs, &and_node, left_rest, config)?;
    let right_tail = build_and_tail(exprs, logic_exprs, &and_node, right_rest, config)?;

    let or_expr = build_commutative_expr(
        exprs,
        logic_exprs,
        &or_node,
        LogicExprKey::Or,
        |inputs| LogicExprKind::Or(inputs),
        vec![left_tail, right_tail],
        config.share_logic_flattening_limit,
        canonicalize_or_operands,
    )?;

    let and_expr = build_commutative_expr(
        exprs,
        logic_exprs,
        &and_node,
        LogicExprKey::And,
        |inputs| LogicExprKind::And(inputs),
        vec![common, or_expr],
        config.share_logic_flattening_limit,
        canonicalize_and_operands,
    )?;

    Ok(Some(and_expr))
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

    build_commutative_expr(
        exprs,
        logic_exprs,
        node,
        LogicExprKey::And,
        |inputs| LogicExprKind::And(inputs),
        operands,
        config.share_logic_flattening_limit,
        canonicalize_and_operands,
    )
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
    build_keyed_expr(exprs, logic_exprs, node, key, kind_ctor(operands), enable_sharing)
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
        LogicExprKind::And(inputs)
        | LogicExprKind::Or(inputs)
        | LogicExprKind::Xor(inputs) => inputs.clone(),
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