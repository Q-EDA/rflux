.subckt leaf in out params: rval=10 cval=1p
R1 in out rval
C1 out 0 cval
.ends

.subckt stage in out params: stage_r=10 stage_c=1p
Xleaf in out leaf params: rval=stage_r cval=stage_c
.ends