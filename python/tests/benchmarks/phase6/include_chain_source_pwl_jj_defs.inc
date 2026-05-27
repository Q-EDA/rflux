.include "include_chain_source_pwl_jj_leaf.inc"

.subckt source_pwl_jj_stage out params: stage_r=10
Xleaf out source_pwl_jj_leaf params: rdrive=stage_r
.ends