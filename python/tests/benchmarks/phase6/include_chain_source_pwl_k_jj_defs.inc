.include "include_chain_source_pwl_k_jj_leaf.inc"

.subckt source_pwl_coupled_jj_stage out tap params: stage_k=0.9 stage_l=1p stage_r=10
Xleaf out tap source_pwl_coupled_jj_leaf params: coupling=stage_k lval=stage_l rval=stage_r
.ends