// Temporary for e2e-test PRs
#![allow(dead_code)]

#[cfg(test)]
mod container;
#[cfg(test)]
mod templates;

#[cfg(test)]
mod tests {
    #[test]
    fn it_works() {
        let result = 2 + 2;
        assert_eq!(result, 4);
    }
}
