.subckt source_coupled_leaf out tap params: coupling=0.9 lval=1p rval=1
V1 in 0 PULSE(0,1m,0,1p,1p,2p,8p)
K1 L1 L2 coupling=coupling
L1 in out lval
R1 out 0 rval
L2 tap 0 lval
R2 tap 0 rval
.ends