.include "include_chain_delay_source_pwl_k_jj_leaf.inc"

.subckt delay_source_pwl_coupled_jj_stage out tap params: stage_z0=50 stage_td=3p stage_k=0.9 stage_l=1p stage_r=10
Xleaf out tap delay_source_pwl_coupled_jj_leaf params: z0=stage_z0 td=stage_td coupling=stage_k lval=stage_l rval=stage_r
.ends