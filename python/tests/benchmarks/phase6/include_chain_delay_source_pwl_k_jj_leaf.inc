.model jjmod jj(icrit=0.5m rn=20 cj=0.5p)

.subckt delay_source_pwl_coupled_jj_leaf out tap params: z0=50 td=3p coupling=0.9 lval=1p rval=10
V1 src 0 PWL(file="include_chain_delay_source_pwl_k_jj_leaf_wave.txt")
T1 src 0 mid 0 z0=z0 td=td
K1 L1 L2 coupling=coupling
L1 mid out lval
R1 out n1 rval
J1 n1 0 jjmod
L2 tap 0 lval
R2 tap 0 rval
.ends