.model jjmod jj(icrit=0.5m rn=20 cj=0.5p)

.subckt source_pwl_coupled_jj_leaf out tap params: coupling=0.9 lval=1p rval=10
V1 in 0 PWL(file="include_chain_source_pwl_k_jj_leaf_wave.txt")
K1 L1 L2 coupling=coupling
L1 in out lval
R1 out n1 rval
J1 n1 0 jjmod
L2 tap 0 lval
R2 tap 0 rval
.ends