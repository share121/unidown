pub fn sanitize(filename: &str) -> String {
    sanitize_filename::sanitize_with_options(
        filename,
        sanitize_filename::Options {
            windows: cfg!(windows),
            truncate: true,
            replacement: "_",
        },
    )
}
