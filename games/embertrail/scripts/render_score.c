/* Offline score renderer. Original additive instruments; no runtime synthesis. */
#include <math.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#define RATE 44100
#define PI 3.14159265358979323846
static uint32_t rng=0x7f4a7c15;
static float noise(void){rng^=rng<<13;rng^=rng>>17;rng^=rng<<5;return (rng/(double)UINT32_MAX)*2-1;}
static void u16(FILE*f,uint16_t x){fputc(x&255,f);fputc(x>>8,f);}
static void u32(FILE*f,uint32_t x){u16(f,x&65535);u16(f,x>>16);}
int main(int argc,char**argv){
 if(argc!=5)return 2;
 size_t n=(size_t)llround(atof(argv[2])*RATE);
 float *l=calloc(n,sizeof(float)),*r=calloc(n,sizeof(float));
 if(!l||!r)return 3;
 FILE*in=fopen(argv[1],"r"); if(!in)return 4;
 double at,dur,midi,amp,pan;int voice;
 while(fscanf(in,"%lf %lf %lf %lf %lf %d",&at,&dur,&midi,&amp,&pan,&voice)==6){
  double freq=440*pow(2,(midi-69)/12),release=voice==0?.12:voice==1?.5:voice==2?1.3:voice==3?.16:voice==4?.3:voice==5?.65:voice==6?.16:voice==7?.8:.25;
  size_t start=(size_t)llround(at*RATE),length=(size_t)((dur+release)*RATE);
  double gl=sqrt((1-pan)*.5)*amp,gr=sqrt((1+pan)*.5)*amp;
  for(size_t i=0;i<length;i++){
   double t=i/(double)RATE,env=1,x=0,p=2*PI*freq*t;
   if(voice<8){
    double attack=voice==2?.025:voice==5?.045:.006;
    env=fmin(1,t/attack)*(t<dur?1:exp(-(t-dur)*6/release));
    switch(voice){
     case 0:x=sin(p)*.75+sin(2*p)*.16+sin(3*p)*.07;env*=exp(-t*1.5);break; /* wooden flute */
     case 1:x=sin(p)*.6+sin(2*p)*.24+sin(3*p)*.11+sin(4*p)*.05;env*=exp(-t*5);break; /* lute */
     case 2:x=sin(p)*.7+sin(2.004*p)*.18+sin(3.997*p)*.09;env*=exp(-t*2.8);break; /* glass */
     case 3:x=sin(p)*.68+sin(2*p)*.2+sin(4*p)*.06;break; /* reed */
     case 4:x=sin(p)*.65+sin(2*p)*.2+sin(3*p)*.1+sin(5*p)*.04;env*=.8+.2*sin(2*PI*5*t);break; /* horn */
     case 5:x=sin(p+.012*sin(2*PI*5*t))*.68+sin(2*p)*.18+sin(3*p)*.08;break; /* bowed strings */
     case 6:x=sin(p)*.86+sin(2*p)*.12;env*=exp(-t*2);break; /* round bass */
     case 7:x=sin(p)*.57+sin(2.76*p)*.22+sin(5.4*p)*.09;env*=exp(-t*4);break; /* hammered bell */
    }
   }else if(voice==8){x=sin(2*PI*(55*t+28*(1-exp(-t*24))/24));env=exp(-t*15);} /* kick */
   else if(voice==9){x=noise()*.65+sin(2*PI*180*t)*.35;env=exp(-t*27);} /* brush/snare */
   else if(voice==10){x=noise();env=exp(-t*70)*.3;} /* shaker */
   else {x=sin(2*PI*(freq*t+15*(1-exp(-t*13))/13))*.85+noise()*.15;env=exp(-t*7);} /* frame drum */
   size_t j=(start+i)%n;l[j]+=x*env*gl;r[j]+=x*env*gr;
  }
 }
 fclose(in);
 /* Circular early reflections retain musical tails across the exact loop boundary. */
 float *dl=malloc(n*sizeof(float)),*dr=malloc(n*sizeof(float));
 memcpy(dl,l,n*sizeof(float));memcpy(dr,r,n*sizeof(float));
 const int delays[]={6173,10373,15971};const float gains[]={.15,.09,.045};
 for(int d=0;d<3;d++)for(size_t i=0;i<n;i++){size_t j=(i+n-delays[d])%n;l[i]+=dr[j]*gains[d];r[i]+=dl[j]*gains[d];}
 free(dl);free(dr);
 double dc[2]={0,0},peak=0;
 for(size_t i=0;i<n;i++){dc[0]+=l[i];dc[1]+=r[i];}dc[0]/=n;dc[1]/=n;
 for(size_t i=0;i<n;i++){l[i]-=dc[0];r[i]-=dc[1];peak=fmax(peak,fmax(fabs(l[i]),fabs(r[i])));}
 double gain=.76/fmax(.76,peak),rms=0,outdc[2]={0,0},jump=fmax(fabs(l[0]-l[n-1]),fabs(r[0]-r[n-1]))*gain;
 FILE*out=fopen(argv[3],"wb");if(!out)return 5;
 fwrite("RIFF",1,4,out);u32(out,36+n*4);fwrite("WAVEfmt ",1,8,out);u32(out,16);u16(out,1);u16(out,2);u32(out,RATE);u32(out,RATE*4);u16(out,4);u16(out,16);fwrite("data",1,4,out);u32(out,n*4);
 for(size_t i=0;i<n;i++){int16_t a=lrint(l[i]*gain*32767),b=lrint(r[i]*gain*32767);u16(out,a);u16(out,b);double x=a/32768.,y=b/32768.;rms+=x*x+y*y;outdc[0]+=x;outdc[1]+=y;}
 fclose(out);free(l);free(r);
 in=fopen(argv[3],"rb");uint64_t hash=UINT64_C(0xcbf29ce484222325);int c;while((c=fgetc(in))!=EOF){hash^=c;hash*=UINT64_C(0x100000001b3);}fclose(in);
 out=fopen(argv[4],"w");fprintf(out,"{\"file\":\"song.wav\",\"checksum\":\"%016llx\",\"frames\":%zu,\"peak\":%.9f,\"rms\":%.9f,\"dc\":%.9f,\"seam_jump\":%.9f}",(unsigned long long)hash,n,peak*gain,sqrt(rms/(2*n)),fmax(fabs(outdc[0]),fabs(outdc[1]))/n,jump);fclose(out);return 0;
}
