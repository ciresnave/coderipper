pub fn total(repo: &impl Repo) -> u32 { repo.a() + repo.b() + repo.c() + repo.d() }
#[cfg(test)]
mod tests {
    #[test]
    fn total_adds_the_parts() {
        let mut repo = MockRepo::new();
        repo.expect_a().returning(|| 1);
        repo.expect_b().returning(|| 2);
        repo.expect_c().returning(|| 3);
        repo.expect_d().returning(|| 4);
        assert_eq!(super::total(&repo), 10);
    }
}
