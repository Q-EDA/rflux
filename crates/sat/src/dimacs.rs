//! DIMACS CNF parsing.
//!
//! The parser is the strict, hand-written parser historically shipped with
//! `rflux-sat`. It skips `c` comment lines and blank lines, expects a single
//! `p cnf <vars> <clauses>` header, and verifies that the declared clause count
//! matches the number of clauses actually parsed.

use crate::types::{CnfFormula, Lit, SatError};

/// Parse a DIMACS CNF string into a [`CnfFormula`].
pub(crate) fn parse_dimacs(input: &str) -> Result<CnfFormula, SatError> {
    let mut expected_clause_count = None::<usize>;
    let mut formula = None::<CnfFormula>;
    let mut pending_clause = Vec::new();
    let mut parsed_clause_count = 0usize;

    for raw_line in input.lines() {
        let line = raw_line.trim();
        if line.is_empty() || line.starts_with('c') {
            continue;
        }

        if line.starts_with('p') {
            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() != 4 || parts[1] != "cnf" {
                return Err(SatError::InvalidDimacsHeader(line.to_string()));
            }
            let parsed_var_count = parts[2]
                .parse::<usize>()
                .map_err(|_| SatError::InvalidDimacsHeader(line.to_string()))?;
            let parsed_clause_count = parts[3]
                .parse::<usize>()
                .map_err(|_| SatError::InvalidDimacsHeader(line.to_string()))?;
            expected_clause_count = Some(parsed_clause_count);
            formula = Some(CnfFormula::new(parsed_var_count));
            continue;
        }

        let Some(ref mut cnf) = formula else {
            return Err(SatError::MissingDimacsHeader);
        };

        for token in line.split_whitespace() {
            let lit = token
                .parse::<i32>()
                .map_err(|_| SatError::InvalidDimacsLiteral(token.to_string()))?;
            if lit == 0 {
                if pending_clause.is_empty() {
                    return Err(SatError::EmptyClause);
                }
                cnf.add_clause(std::mem::take(&mut pending_clause))?;
                parsed_clause_count += 1;
                continue;
            }
            let var = lit.unsigned_abs() as usize;
            if var == 0 || var > cnf.var_count() {
                return Err(SatError::VariableOutOfRange {
                    var,
                    var_count: cnf.var_count(),
                });
            }
            pending_clause.push(if lit > 0 {
                Lit::pos(var)
            } else {
                Lit::neg(var)
            });
        }
    }

    let Some(cnf) = formula else {
        return Err(SatError::MissingDimacsHeader);
    };

    if !pending_clause.is_empty() {
        return Err(SatError::UnterminatedDimacsClause);
    }

    let expected_clause_count = expected_clause_count.ok_or(SatError::MissingDimacsHeader)?;
    if parsed_clause_count != expected_clause_count {
        return Err(SatError::InvalidDimacsClauseCount {
            expected: expected_clause_count,
            actual: parsed_clause_count,
        });
    }

    Ok(cnf)
}
