#![cfg_attr(not(feature = "std"), no_std, no_main)]

#[ink::contract]
mod simple_storage {
    use ink::storage::Mapping;

    /// Simple storage contract for SaaS demo.
    /// Demonstrates basic state management with ownership.
    #[ink(storage)]
    pub struct SimpleStorage {
        /// Owner of the contract
        owner: AccountId,
        /// Stored value
        value: u32,
        /// Mapping of user balances (example for multi-tenant SaaS)
        balances: Mapping<AccountId, u128>,
        /// Total supply
        total_supply: u128,
    }

    /// Events
    #[ink(event)]
    pub struct ValueChanged {
        #[ink(topic)]
        new_value: u32,
    }

    #[ink(event)]
    pub struct Transfer {
        #[ink(topic)]
        from: Option<AccountId>,
        #[ink(topic)]
        to: Option<AccountId>,
        value: u128,
    }

    /// Errors
    #[derive(Debug, PartialEq, Eq, scale::Encode, scale::Decode)]
    #[cfg_attr(feature = "std", derive(scale_info::TypeInfo))]
    pub enum Error {
        /// Caller is not the owner
        NotOwner,
        /// Insufficient balance
        InsufficientBalance,
        /// Overflow
        Overflow,
    }

    pub type Result<T> = core::result::Result<T, Error>;

    impl SimpleStorage {
        /// Constructor
        #[ink(constructor)]
        pub fn new(initial_value: u32) -> Self {
            let caller = Self::env().caller();
            Self {
                owner: caller,
                value: initial_value,
                balances: Mapping::default(),
                total_supply: 0,
            }
        }

        /// Default constructor
        #[ink(constructor)]
        pub fn default() -> Self {
            Self::new(0)
        }

        /// Get the current value
        #[ink(message)]
        pub fn get(&self) -> u32 {
            self.value
        }

        /// Set a new value (only owner)
        #[ink(message)]
        pub fn set(&mut self, new_value: u32) -> Result<()> {
            self.ensure_owner()?;
            self.value = new_value;
            self.env().emit_event(ValueChanged { new_value });
            Ok(())
        }

        /// Get owner
        #[ink(message)]
        pub fn owner(&self) -> AccountId {
            self.owner
        }

        /// Mint tokens to an account (only owner) - SaaS credit example
        #[ink(message)]
        pub fn mint(&mut self, to: AccountId, amount: u128) -> Result<()> {
            self.ensure_owner()?;
            let balance = self.balances.get(to).unwrap_or(0);
            let new_balance = balance.checked_add(amount).ok_or(Error::Overflow)?;
            self.balances.insert(to, &new_balance);
            self.total_supply = self.total_supply.checked_add(amount).ok_or(Error::Overflow)?;
            self.env().emit_event(Transfer {
                from: None,
                to: Some(to),
                value: amount,
            });
            Ok(())
        }

        /// Transfer tokens
        #[ink(message)]
        pub fn transfer(&mut self, to: AccountId, amount: u128) -> Result<()> {
            let from = self.env().caller();
            let from_balance = self.balances.get(from).unwrap_or(0);
            if from_balance < amount {
                return Err(Error::InsufficientBalance);
            }
            let new_from = from_balance - amount;
            let to_balance = self.balances.get(to).unwrap_or(0);
            let new_to = to_balance.checked_add(amount).ok_or(Error::Overflow)?;

            self.balances.insert(from, &new_from);
            self.balances.insert(to, &new_to);

            self.env().emit_event(Transfer {
                from: Some(from),
                to: Some(to),
                value: amount,
            });
            Ok(())
        }

        /// Get balance of an account
        #[ink(message)]
        pub fn balance_of(&self, account: AccountId) -> u128 {
            self.balances.get(account).unwrap_or(0)
        }

        /// Get total supply
        #[ink(message)]
        pub fn total_supply(&self) -> u128 {
            self.total_supply
        }

        fn ensure_owner(&self) -> Result<()> {
            if self.env().caller() != self.owner {
                return Err(Error::NotOwner);
            }
            Ok(())
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[ink::test]
        fn default_works() {
            let contract = SimpleStorage::default();
            assert_eq!(contract.get(), 0);
        }

        #[ink::test]
        fn set_works() {
            let mut contract = SimpleStorage::new(10);
            assert_eq!(contract.get(), 10);
            assert!(contract.set(42).is_ok());
            assert_eq!(contract.get(), 42);
        }

        #[ink::test]
        fn mint_and_transfer_works() {
            let accounts = ink::env::test::default_accounts::<ink::env::DefaultEnvironment>();
            let mut contract = SimpleStorage::new(0);

            // Mint to Alice
            assert!(contract.mint(accounts.alice, 100).is_ok());
            assert_eq!(contract.balance_of(accounts.alice), 100);
            assert_eq!(contract.total_supply(), 100);

            // Transfer from Alice to Bob (need to set caller)
            ink::env::test::set_caller::<ink::env::DefaultEnvironment>(accounts.alice);
            assert!(contract.transfer(accounts.bob, 30).is_ok());
            assert_eq!(contract.balance_of(accounts.alice), 70);
            assert_eq!(contract.balance_of(accounts.bob), 30);
        }
    }
}
