.include "include_chain_jj_leaf.inc"

.subckt jj_stage in out params: stage_r=10
Xleaf in out jj_leaf params: rdrive=stage_r
.ends