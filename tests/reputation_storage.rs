#[cfg(test)]
mod test {
    use super::*;
    use soroban_sdk::{testutils::Address as _, Address, Env};

    #[test]
    fn test_reputation_storage_roundtrip() {
        let env = Env::default();
        let user = Address::generate(&env);

        let absent_rep = read_reputation(&env, user.clone());
        assert_eq!(absent_rep, None, "Expected absent key to return None");

        let expected_reputation = 250;
        write_reputation(&env, user.clone(), expected_reputation);
        
        let retrieved_rep = read_reputation(&env, user.clone());
        assert_eq!(
            retrieved_rep, 
            Some(expected_reputation), 
            "Expected retrieved reputation to match written value"
        );
    }
}