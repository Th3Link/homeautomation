$fn=100;

use <../library/din_clip_01.scad>

pcb = [80+1,86+1,2];
holes_d = [72,78];
h0 = [(pcb[0]-holes_d[0])/2,(pcb[1]-holes_d[1])/2,[-10,-10]];
clip_w = 15;
holes0 = [
    h0,
    [h0[0],h0[1]+holes_d[1]]
];
holes1 = [
    [h0[0]+holes_d[0],h0[1]+holes_d[1]],
    [h0[0]+holes_d[0],h0[1]],
];
holes = [
    h0,
    [h0[0]+holes_d[0],h0[1]+holes_d[1],[10,10]],
    [h0[0]+holes_d[0],h0[1],[10,-10]],
    [h0[0],h0[1]+holes_d[1],[-10,10]],
];


t1 = 3;
t2 = 2*t1;
bore_r = 2.8/2;
height = 8;
standoff = 10;
foot = 3;
module bottom() {
intersection() {
difference() {
union() {
difference() {
    translate([-t1,-t1,-t1+1]) cube([pcb[0]+2*t1, pcb[1]+2*t1,standoff+pcb[2]+t1-1]);
    cube([pcb[0]+t1, pcb[1],standoff+pcb[2]]);
    translate([-t1,30,7]) cube([pcb[0]+t1,10,standoff+pcb[2]]);
}

for ( hole = holes0 ){
    hull() {
        translate([hole[0], hole[1]+3,0]) cylinder(r2=2*bore_r,r1=standoff,h=standoff);
        translate([hole[0], hole[1]-3,0]) cylinder(r2=2*bore_r,r1=standoff,h=standoff);
        translate([hole[0]-10, hole[1]+3,0]) cylinder(r2=2*bore_r,r1=standoff,h=standoff);
        translate([hole[0]-10, hole[1]-3,0]) cylinder(r2=2*bore_r,r1=standoff,h=standoff);
    }
}

for ( hole = holes1 ){
    hull() {
        translate([hole[0], hole[1]+3,0]) cylinder(r2=2*bore_r,r1=standoff,h=standoff);
        translate([hole[0], hole[1]-3,0]) cylinder(r2=2*bore_r,r1=standoff,h=standoff);
        translate([hole[0]+10, hole[1]+3,0]) cylinder(r2=2*bore_r,r1=standoff,h=standoff);
        translate([hole[0]+10, hole[1]-3,0]) cylinder(r2=2*bore_r,r1=standoff,h=standoff);
    }
}

for ( hole = holes ){
    hull() {
        translate([hole[0], hole[1]+3,0]) cylinder(r2=2*bore_r,r1=standoff,h=standoff);
        translate([hole[0], hole[1]-3,0]) cylinder(r2=2*bore_r,r1=standoff,h=standoff);
    }
}
}
for ( hole = holes ){
        translate([hole[0], hole[1],-2]) cylinder(r=bore_r,h=standoff+t1);
    }
}
translate([-t1,-t1,-t1+1]) cube([pcb[0]+t1, pcb[1]+2*t1,t2+standoff]);
}

translate([(15/2)-t1,0,0])rotate([0,90,0]) din_clip(clip_w);

//hanging clib
translate([pcb[0]-2*clip_w,0,0]) difference() {
translate([clip_w,0,0]) rotate([0,90,0]) din_clip(clip_w*2);
translate([0,0,0]) rotate([0,40,0]) translate([0,0,-clip_w*3]) cube([clip_w*3,pcb[1],clip_w*3]);
}
}

module lamps(r=2,h=6) {
    for (i= [0 : 7])
     {
         translate([i*5,0,0]) cylinder(r=r,h=h);
     }
}

module lid() {
translate([0,0,30]) {
    difference() {
        union() {
translate([-t1,-t1,0]) cube([pcb[0]+t1, pcb[1]+2*t1,7]);
translate([pcb[0],-t1,-9]) cube([2, pcb[1]+2*t1,16]);
        }
        translate([-t1,30,-13]) cube([pcb[0]+2*t1,10,standoff+pcb[2]]);
for ( hole = holes ){
        translate([hole[0], hole[1],0]) cylinder(r=bore_r+0.6,h=7);
    }
    
    //connectors
    translate([17,17,0]) cube([60, 64,7]);
    translate([1,64,0]) cube([60, 17,7]);
    //component hight
    translate([0,0,0]) cube([pcb[0], pcb[1],5]);
}
intersection() {
    for ( hole = holes ){
        difference() {
            hull() {
                translate([hole[0], hole[1],0]) cylinder(r=7/2,h=7);
                translate([hole[0]+hole[2][0], hole[1],0]) cylinder(r=7/2,h=7);
                translate([hole[0], hole[1]+hole[2][1],0]) cylinder(r=7/2,h=7);
                translate([hole[0]+hole[2][0], hole[1]+hole[2][1],0]) cylinder(r=7/2,h=7);
            }
            translate([hole[0], hole[1],0]) cylinder(r=bore_r+0.6,h=7);
        }
    }
translate([-t1,-t1,0]) cube([pcb[0]+t1, pcb[1]+2*t1,7]);
}
}
}
//bottom();
lid();