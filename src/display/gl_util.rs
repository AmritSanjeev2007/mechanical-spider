use glow::HasContext;

/// Compiles and links a vertex + fragment shader pair.
pub fn compile_program(
    gl: &glow::Context,
    vertex_src: &str,
    fragment_src: &str,
) -> Result<glow::Program, String> {
    unsafe {
        let program = gl.create_program()?;
        let mut shaders = Vec::with_capacity(2);
        for (kind, src) in [
            (glow::VERTEX_SHADER, vertex_src),
            (glow::FRAGMENT_SHADER, fragment_src),
        ] {
            let shader = gl.create_shader(kind)?;
            gl.shader_source(shader, src);
            gl.compile_shader(shader);
            if !gl.get_shader_compile_status(shader) {
                let log = gl.get_shader_info_log(shader);
                gl.delete_shader(shader);
                gl.delete_program(program);
                let stage = if kind == glow::VERTEX_SHADER { "vertex" } else { "fragment" };
                return Err(format!("{stage} shader failed to compile:\n{log}"));
            }
            gl.attach_shader(program, shader);
            shaders.push(shader);
        }
        gl.link_program(program);
        let linked = gl.get_program_link_status(program);
        for shader in shaders {
            gl.detach_shader(program, shader);
            gl.delete_shader(shader);
        }
        if !linked {
            let log = gl.get_program_info_log(program);
            gl.delete_program(program);
            return Err(format!("shader program failed to link:\n{log}"));
        }
        Ok(program)
    }
}

/// Reads a vertex and fragment shader from disk (paths relative to the working
/// directory, like `./shaders/...`) and links them into a program.
pub fn compile_program_from_files(
    gl: &glow::Context,
    vertex_path: &str,
    fragment_path: &str,
) -> Result<glow::Program, String> {
    let read = |path: &str| {
        std::fs::read_to_string(path).map_err(|e| format!("failed to load shader {path}: {e}"))
    };
    let vertex_src = read(vertex_path)?;
    let fragment_src = read(fragment_path)?;
    compile_program(gl, &vertex_src, &fragment_src)
        .map_err(|e| format!("{vertex_path} + {fragment_path}: {e}"))
}
