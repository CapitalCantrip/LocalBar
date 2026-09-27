pub fn tokens(command_line: &str) -> Vec<&str> {
    command_line.split_whitespace().collect()
}

pub fn basename(token: &str) -> &str {
    token.rsplit('/').next().unwrap_or(token)
}

pub fn executable_is(tokens: &[&str], name: &str) -> bool {
    tokens.first().is_some_and(|t| basename(t) == name)
}

pub fn has_adjacent_pair(tokens: &[&str], first: &str, second: &str) -> bool {
    tokens.windows(2).any(|w| w[0] == first && w[1] == second)
}
