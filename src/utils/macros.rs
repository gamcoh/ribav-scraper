// extract!(post, &author_sel, text) -> post.select(&author_sel).next().unwrap().text().collect::<String>().trim().to_string()
#[macro_export]
macro_rules! extract {
    ($post:ident, $sel:expr) => {
        $post
            .select($sel)
            .next()
            .unwrap()
            .text()
            .collect::<String>()
            .trim()
            .to_string()
    };
    ($post:ident, $sel:expr, $want:ident) => {
        $post.select($sel).next().unwrap().$want()
    };
}
