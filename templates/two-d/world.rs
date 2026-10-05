#[cfg(feature="client")]
impl draw::Game for Garden {
    fn draw(&self,s:&mut draw::Scene) {
        use draw::*;
        let mut world=World::new([4.,5.,5.5],[4.,0.,2.]);
        world.cube([4.,-0.2,2.],[8.,0.2,4.],Color::new(0.12,0.22,0.24,1.));
        for r in WALLS {world.cube([(r.x+r.w/2) as f32/100.,0.2,(r.y+r.h/2) as f32/100.],[r.w as f32/100.,0.5,r.h as f32/100.],Color::new(0.3,0.39,0.42,1.));}
        for n in 0..4 {if self.state.collected&(1<<n)==0 {world.sphere([(172+n*140) as f32/100.,0.3,1.1],0.12,GOLD);}}
        world.cube([7.37,0.1,1.1],[0.55,0.2,0.7],TEAL);
        world.sphere([(317+self.state.tick as i32%200) as f32/100.,0.25,2.27],0.2,PINK);
        let r=self.state.player;world.cube([(r.x+12) as f32/100.,0.22,(r.y+12) as f32/100.],[0.24,0.45,0.24],WHITE);
        s.world(0,Rect::new(20,55,760,345),world);
        s.text(20,format!("Lanterns {}/4   Hearts {}",self.state.collected.count_ones(),self.state.hearts),Point::new(360,30),22.,GOLD);
        // {{minimap}}
    }
}
