//! Synthetic Sylvaculture — reproduction of
//! Makowski et al. 2019, "Synthetic Silviculture: Multi-scale Modeling of Plant Ecosystems".
//!
//! Milestone 0: validate the three-d 0.19 viewer (window, orbit camera, ground, lights,
//! a placeholder trunk). The growth model gets wired in once this renders.

use three_d::*;

fn main() {
    let window = Window::new(WindowSettings {
        title: "Synthetic Sylvaculture".to_string(),
        max_size: Some((1280, 800)),
        ..Default::default()
    })
    .unwrap();
    let context = window.gl();

    let mut camera = Camera::new_perspective(
        window.viewport(),
        vec3(6.0, 4.0, 6.0),
        vec3(0.0, 2.0, 0.0),
        vec3(0.0, 1.0, 0.0),
        degrees(45.0),
        0.1,
        1000.0,
    );
    let mut control = OrbitControl::new(camera.target(), 1.0, 100.0);

    // --- lights ---
    let ambient = AmbientLight::new(&context, 0.5, Srgba::WHITE);
    let directional =
        DirectionalLight::new(&context, 2.0, Srgba::WHITE, vec3(-0.5, -1.0, -0.7));

    // --- ground plane (flattened, lifted square in the xz-plane) ---
    let mut ground = Gm::new(
        Mesh::new(&context, &CpuMesh::square()),
        PhysicalMaterial::new_opaque(
            &context,
            &CpuMaterial {
                albedo: Srgba::new(70, 110, 60, 255),
                ..Default::default()
            },
        ),
    );
    ground.set_transformation(
        Mat4::from_angle_x(degrees(-90.0)) * Mat4::from_scale(50.0),
    );

    // --- placeholder trunk: unit cylinder runs along +X, rotate it to +Y ---
    let mut trunk = Gm::new(
        Mesh::new(&context, &CpuMesh::cylinder(16)),
        PhysicalMaterial::new_opaque(
            &context,
            &CpuMaterial {
                albedo: Srgba::new(120, 80, 50, 255),
                ..Default::default()
            },
        ),
    );
    trunk.set_transformation(
        Mat4::from_angle_z(degrees(90.0)) * Mat4::from_nonuniform_scale(3.0, 0.15, 0.15),
    );

    window.render_loop(move |mut frame_input| {
        camera.set_viewport(frame_input.viewport);
        control.handle_events(&mut camera, &mut frame_input.events);

        frame_input
            .screen()
            .clear(ClearState::color_and_depth(0.55, 0.68, 0.85, 1.0, 1.0))
            .render(
                &camera,
                ground.into_iter().chain(&trunk),
                &[&ambient, &directional],
            );

        FrameOutput::default()
    });
}
