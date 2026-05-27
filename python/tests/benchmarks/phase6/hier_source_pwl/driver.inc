.subckt driver out params: rval=10 cval=1p
V1 src 0 PWL(file="driver_wave.txt")
R1 src out rval
C1 out 0 cval
.ends