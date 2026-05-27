.include "include_chain_delay_source_pwl_jj_leaf.inc"

.subckt delay_source_jj_stage out params: stage_z0=50 stage_td=3p stage_r=10
Xleaf out delay_source_jj_leaf params: z0=stage_z0 td=stage_td rdrive=stage_r
.ends