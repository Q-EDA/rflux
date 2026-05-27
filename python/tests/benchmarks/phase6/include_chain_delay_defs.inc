.include "include_chain_delay_leaf.inc"

.subckt delay_stage in out params: stage_z0=50 stage_td=3p stage_r=50
Xleaf in out delay_leaf params: z0=stage_z0 td=stage_td rval=stage_r
.ends