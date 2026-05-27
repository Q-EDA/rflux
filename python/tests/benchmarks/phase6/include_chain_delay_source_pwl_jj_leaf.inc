.model jjmod jj(icrit=0.5m rn=20 cj=0.5p)

.subckt delay_source_jj_leaf out params: z0=50 td=3p rdrive=10
V1 src 0 PWL(file="include_chain_delay_source_pwl_jj_leaf_wave.txt")
T1 src 0 mid 0 z0=z0 td=td
R1 mid n1 rdrive
J1 n1 out jjmod
.ends