.model jjmod jj(icrit=0.5m rn=20 cj=0.5p)

.subckt delay_jj_leaf out params: z0=50 td=3p rdrive=10
V1 src 0 PULSE(0,2m,0,1p,1p,2p,8p)
T1 src 0 mid 0 z0=z0 td=td
R1 mid n1 rdrive
J1 n1 out jjmod
.ends