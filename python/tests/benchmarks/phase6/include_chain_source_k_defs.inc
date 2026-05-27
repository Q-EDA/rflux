.include "include_chain_source_k_leaf.inc"

.subckt source_coupled_stage out tap params: stage_k=0.9 stage_l=1p stage_r=1
Xleaf out tap source_coupled_leaf params: coupling=stage_k lval=stage_l rval=stage_r
.ends