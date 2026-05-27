.model jjmod jj(icrit=0.5m rn=20 cj=0.5p)

.subckt source_pwl_jj_leaf out params: rdrive=10
V1 in 0 PWL(file="include_chain_source_pwl_jj_leaf_wave.txt")
R1 in n1 rdrive
J1 n1 out jjmod
.ends