.model jjmod jj(icrit=0.5m rn=20 cj=0.5p)

.subckt source_jj_leaf out params: rdrive=10
V1 in 0 PULSE(0,2m,0,1p,1p,2p,8p)
R1 in n1 rdrive
J1 n1 out jjmod
.ends