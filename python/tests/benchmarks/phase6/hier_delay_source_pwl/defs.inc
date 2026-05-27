.subckt delay_driver out params: z0=50 td=3p rval=50
V1 src 0 PWL(file="driver_wave.txt")
T1 src 0 out 0 z0=z0 td=td
R1 out 0 rval
.ends