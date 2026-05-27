.subckt driver out params: rval=10 cval=1p
V1 src 0 PULSE(0 1m 1p 0.2p 0.2p 2p 4p)
R1 src out rval
C1 out 0 cval
.ends