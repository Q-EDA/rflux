.include "leaf.inc"

.subckt stage out params: stage_r=10 stage_c=1p
Xleaf out leaf params: rval=stage_r cval=stage_c
.ends