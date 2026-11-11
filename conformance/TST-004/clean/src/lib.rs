pub fn total(repo: &impl Repo) -> u32 { repo.a() + repo.b() }
#[cfg(test)]
mod tests {
    #[test]
    fn total_adds_the_parts() {
        let mut repo = MockRepo::new();
        repo.expect_a().returning(|| 1);
        repo.expect_b().returning(|| 2);
        assert_eq!(super::total(&repo), 3);
    }
    #[test]
    fn total_of_zero_parts() {
        let mut repo = MockRepo::new();
        repo.expect_a().returning(|| 0);
        repo.expect_b().returning(|| 0);
        repo.expect_c().returning(|| 0);
        repo.expect_d().returning(|| 0);
        assert_eq!(super::total(&repo), 0);
        assert!(repo.is_quiet());
        assert!(repo.is_clean());
        assert!(repo.is_empty());
    }
}
