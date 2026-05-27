.include "include_chain_k_leaf.inc"

.subckt coupled_stage in out tap params: stage_k=0.9 stage_l=1p stage_r=1
Xleaf in out tap coupled_leaf params: coupling=stage_k lval=stage_l rval=stage_r
.ends