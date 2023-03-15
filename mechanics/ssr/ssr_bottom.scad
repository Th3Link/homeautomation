$fn=100;

use <../library/din_clip_01.scad>

pcb = [112+1,90+1,2];
holes_d = [106,70];
h0 = [(pcb[0]-holes_d[0])/2,(pcb[1]-holes_d[1])/2-1.5];
clip_w = 15;
holes = [
    h0,
    [h0[0]+holes_d[0],h0[1]+holes_d[1]],
    [h0[0]+holes_d[0],h0[1]],
    [h0[0],h0[1]+holes_d[1]],
];


t1 = 3;
t2 = 2*t1;
bore_r = 2.8/2;

module bottom() {
intersection() {
difference() {
union() {
difference() {
    translate([-t1,-t1,-t1+1]) cube([pcb[0]+2*t1, pcb[1]+2*t1,t2+1]);
    cube([pcb[0]+t1, pcb[1],5]);
}

for ( hole = holes ){
    hull() {
        translate([hole[0], hole[1]+3,0]) cylinder(r2=2*bore_r,r1=4*bore_r,h=5-2);
        translate([hole[0], hole[1]-3,0]) cylinder(r2=2*bore_r,r1=4*bore_r,h=5-2);
    }
}
}
for ( hole = holes ){
        translate([hole[0], hole[1],-2]) cylinder(r=bore_r,h=6);
    }
}
translate([-t1,-t1,-t1+1]) cube([pcb[0]+t1, pcb[1]+2*t1,t2+1]);
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
for ( hole = holes ){
        translate([hole[0], hole[1],0]) cylinder(r=bore_r+0.6,h=7);
    }
    //rj45 can
    translate([7,pcb[1]-22+3,0]) cube([34, 19+3,7]);
    //rj45 can
    translate([6,pcb[1]-8+3,0]) cube([36, 19+3,7]);
    //Fuses
    translate([(pcb[0]-23.5)/2,19,0]) cube([22.8, 25,7]);

    //WAGO connectors
    tempx = pcb[0]-14;
    translate([(pcb[0]-tempx)/2,-t1,0]) cube([tempx, 17+t1+0.5,7]);

    //micromatch connectors
    translate([3.5+48,8.5+42,0]) cube([12, 12,7]);

    //component hight
    translate([0,0,0]) cube([pcb[0], pcb[1],5]);
    
    translate([0,8.5+45,0]) union() {
         translate([3.5+10.5-3,0,0]) lamps(r=2,h=7);
         translate([74.5-10.5+3,0,0]) lamps(r=2,h=7);
    }
}
for ( hole = holes ){
difference() {
translate([hole[0], hole[1],0]) cylinder(r=7/2,h=6);
    translate([hole[0], hole[1],0]) cylinder(r=bore_r+0.6,h=6);
}
}
translate([0,8.5+45,0]) difference() {
translate([0,0,1.5]) union() {
 hull() translate([3.5+10.5-3,0,0]) lamps(r=3,h=4.5);
 hull() translate([74.5-10.5+3,0,0]) lamps(r=3,h=4.5);
}
 translate([3.5+10.5-3,0,0]) lamps(r=2,h=6);
 translate([74.5-10.5+3,0,0]) lamps(r=2,h=6);
}
}
}
//bottom();
lid();