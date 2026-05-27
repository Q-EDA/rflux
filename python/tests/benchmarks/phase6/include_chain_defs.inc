.include "include_chain_leaf.inc"

.subckt stage in out params: stage_r=10 stage_c=1p
Xleaf in out leaf params: rval=stage_r cval=stage_c
.ends