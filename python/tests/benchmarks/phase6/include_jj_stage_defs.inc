.model jjmod jj(icrit=0.5m rn=20 cj=0.5p)

.subckt jj_stage in out params: rdrive=10
R1 in n1 rdrive
J1 n1 out jjmod
.ends