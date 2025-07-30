use sp1_build::build_program;

fn main() {
    build_program("./programs/data-correctness");
    build_program("./programs/dawn-sla");
}
