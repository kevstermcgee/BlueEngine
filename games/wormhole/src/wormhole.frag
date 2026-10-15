#version 100
precision highp float;
varying vec2 uv;
uniform vec4 Journey;
uniform vec4 Phase;
uniform vec4 Seeds;
uniform vec4 Camera;
const float TAU=6.28318530718;
float hash(float x) { return fract(sin(x*127.1+311.7)*43758.5453); }
float noise(vec3 p) {
    vec3 i=floor(p), f=fract(p); f=f*f*(3.0-2.0*f);
    float n=dot(i,vec3(1.0,57.0,113.0));
    return mix(mix(mix(hash(n),hash(n+1.0),f.x),mix(hash(n+57.0),hash(n+58.0),f.x),f.y),
               mix(mix(hash(n+113.0),hash(n+114.0),f.x),mix(hash(n+170.0),hash(n+171.0),f.x),f.y),f.z);
}
float seedAt(float z) {
    if(z<0.0) return Seeds.x;
    if(z<160.0) return Seeds.y;
    if(z<320.0) return Seeds.z;
    return Seeds.w;
}
vec4 properties(float seed) { return vec4(hash(seed*821.0),hash(seed*1233.0),hash(seed*473.0),hash(seed*2921.0)); }
vec4 params(float z) {
    float cell=floor(z/160.0), u=fract(z/160.0);
    return mix(properties(seedAt(cell*160.0)),properties(seedAt((cell+1.0)*160.0)),smoothstep(0.0,1.0,u));
}
vec2 axis(float z) {
    float a=z*TAU/160.0;
    return vec2(1.2*sin(a)+0.5*sin(a*2.0),0.8*cos(a)+0.4*sin(a*3.0));
}
float wall(vec3 p) {
    vec4 k=params(p.z);
    vec2 q=p.xy-axis(p.z);
    float a=atan(q.y,q.x), z=p.z*TAU/160.0;
    float rotation=z*2.0+sin(z)*3.0*k.w+Phase.z*1.2;
    float petal=mix(sin(a*3.0+rotation),sin(a*8.0-rotation),k.x);
    float polygon=cos(3.0*a+rotation)*cos(2.0*a-rotation);
    float wave=sin(z*8.0+Phase.x*2.0+sin(a*4.0+z*3.0));
    float square=pow(pow(abs(q.x),4.0)+pow(abs(q.y),4.0),0.25);
    float radius=5.8+1.45*petal*(0.25+k.y)+0.85*polygon*k.z+0.65*wave;
    return (radius-mix(length(q),square,k.w*0.65))*0.42;
}
vec3 palette(float value, vec4 k) {
    float hue=k.z+value*(0.17+k.w*0.52);
    vec3 c=0.52+0.48*cos(TAU*(hue+vec3(0.0,0.34,0.67)));
    return pow(c,vec3(1.3));
}
vec3 surface(vec3 p, float travel) {
    vec4 k=params(p.z);
    vec2 q=p.xy-axis(p.z);
    float a=atan(q.y,q.x), z=p.z*TAU/160.0;
    vec3 coord=vec3(cos(a)*3.4,sin(a)*3.4,sin(z)*3.0);
    float cloud=noise(coord+vec3(k.x*12.0,k.y*8.0,Phase.y));
    float fine=noise(coord*3.0+vec3(Phase.z,k.w*4.0,Phase.x));
    float flow=a*8.0+z*12.0+sin(a*5.0-z*4.0)*k.y*4.0+cloud*7.0+Phase.x*2.0;
    float river=pow(0.5+0.5*sin(flow),14.0);
    float lace=pow(0.5+0.5*cos(flow*2.1+fine*5.0),25.0);
    float splash=pow(0.5+0.5*sin(z*17.0+sin(a*5.0+z*3.0)*2.0+Phase.y),10.0);
    float ribbons=0.5+0.5*sin(a*2.0-z*5.0+cloud*4.0);
    float rings=pow(0.5+0.5*sin(z*23.0+sin(a*2.0)*3.0+Phase.z),12.0);
    float streams=pow(0.5+0.5*sin(a*28.0+z*2.0+cloud*4.0),22.0);
    float pattern=mix(river,rings,smoothstep(0.15,0.85,k.x));
    pattern=mix(pattern,streams,k.w*0.65);
    vec3 c=palette(cloud+ribbons*0.8+splash*0.4,k)*(0.16+pattern*3.0+splash*1.1);
    c+=palette(1.3+fine,k)*lace*0.9;
    c+=vec3(0.65,0.85,1.0)*pow(river,4.0)*0.55;
    float mist=1.0-exp(-travel*0.009);
    return mix(c,palette(0.8,k)*0.20,mist);
}
void main() {
    vec2 st=(uv-0.5)*2.0; st.x*=Journey.y;
    vec3 rd=normalize(vec3(st.x,-st.y,1.16));
    float roll=Phase.w*0.08;
    rd.xy=mat2(cos(roll),-sin(roll),sin(roll),cos(roll))*rd.xy;
    rd.yz=mat2(cos(Camera.y),-sin(Camera.y),sin(Camera.y),cos(Camera.y))*rd.yz;
    rd.xz=mat2(cos(Camera.x),-sin(Camera.x),sin(Camera.x),cos(Camera.x))*rd.xz;
    vec3 ro=vec3(axis(Journey.x)+vec2(Phase.x,Phase.y)*0.22,Journey.x);
    float travel=0.0, d=0.0, glow=0.0;
    vec3 p=ro; bool hit=false;
    for(int i=0;i<76;i++) {
        p=ro+rd*travel; d=wall(p);
        if(d<0.018) { hit=true; break; }
        glow+=exp(-d*7.0)*0.004;
        travel+=clamp(d,0.035,3.6);
        if(travel>145.0) break;
    }
    vec4 k=params(p.z); vec3 col;
    if(hit) {
        col=surface(p,travel);
        float e=0.05;
        vec3 normal=normalize(vec3(wall(p+vec3(e,0,0))-wall(p-vec3(e,0,0)),wall(p+vec3(0,e,0))-wall(p-vec3(0,e,0)),wall(p+vec3(0,0,e))-wall(p-vec3(0,0,e))));
        col*=0.72+0.28*abs(dot(normal,-rd));
        col+=palette(1.7,k)*glow;
    } else { col=palette(0.6,k)*0.17+vec3(0.55,0.75,1.0)*0.11; }
    float a=atan(st.y,st.x), r=length(st);
    float radial=sin(r*22.0+Phase.x*2.0+sin(a*7.0));
    col+=palette(1.1,k)*pow(max(0.0,radial),35.0)*0.018;
    col*=1.0-0.28*smoothstep(0.2,1.8,r);
    col=vec3(1.0)-exp(-col*1.5);
    gl_FragColor=vec4(pow(col,vec3(0.86)),1.0);
}
