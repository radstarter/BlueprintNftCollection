use radix_engine::transaction::{TransactionReceipt};
use scrypto::prelude::*;
use scrypto_unit::*;
use transaction::builder::ManifestBuilder;
use transaction::model::{TransactionManifest};
use transaction::ecdsa_secp256k1::EcdsaSecp256k1PrivateKey;

type Actor = (EcdsaSecp256k1PublicKey, EcdsaSecp256k1PrivateKey, ComponentAddress);

struct TestEnv {
    runner: TestRunner,
    collection: ComponentAddress,
    owner_badge: ResourceAddress
}

impl TestEnv {
    fn new() -> (
        TestEnv,
        Actor,      // seller: key, account
        Vec<Actor>, // buyers: key, account
    ) {
        let mut runner = TestRunner::builder().without_trace().build();
        let seller = runner.new_allocated_account();
        let buyers: Vec<Actor> = (0..3).map(|_| runner.new_allocated_account()).collect();
        let package = runner.compile_and_publish(this_package!());
        
        let transaction = ManifestBuilder::new()
            .call_function(package, "NftProject", "instantiate_component", manifest_args!())
            .call_method(seller.2, "deposit_batch", manifest_args!(ManifestExpression::EntireWorktop))
            .build();
        let receipt = runner.execute_manifest_ignoring_fee(transaction, vec![NonFungibleGlobalId::from_public_key(&seller.0)]);
        receipt.expect_commit_success();
        let result = &receipt.expect_commit(true);
        println!("{:?}\n", result);
        let collection = result.new_component_addresses()[0];
        let owner_badge = result.new_resource_addresses()[0];
        
        (
            TestEnv {
                runner,
                owner_badge,
                collection,
            },
            seller,
            buyers
        )
    }
    
    fn execute(&mut self, transaction: TransactionManifest, actor: &Actor) -> TransactionReceipt {
      self.runner.execute_manifest_ignoring_fee(transaction, vec![NonFungibleGlobalId::from_public_key(&actor.0)])
    }
    
    fn set_fixed_auction(&mut self, actor: &Actor, cost: Decimal){
        let transaction = ManifestBuilder::new()
            .create_proof_from_account_by_amount(actor.2, self.owner_badge, dec!(1))
            .call_method(self.collection,"set_auction_fixed", manifest_args!(cost))
            .call_method(actor.2, "deposit_batch", manifest_args!(ManifestExpression::EntireWorktop))
            .build();
        let receipt = self.execute(transaction, actor);
        receipt.expect_commit_success();
    }
    
    fn set_dutch_auction(&mut self, actor: &Actor, initial: Decimal, decrease: Decimal, length: u64){
        let transaction = ManifestBuilder::new()
            .create_proof_from_account_by_amount(actor.2, self.owner_badge, dec!(1))
            .call_method(self.collection,"set_auction_dutch", manifest_args!(initial, decrease, length))
            .call_method(actor.2, "deposit_batch", manifest_args!(ManifestExpression::EntireWorktop))
            .build();
        let receipt = self.execute(transaction, actor);
        receipt.expect_commit_success();
    }
    
    fn set_english_auction(&mut self, actor: &Actor, initial: Decimal, length: u64){
        let transaction = ManifestBuilder::new()
            .create_proof_from_account_by_amount(actor.2, self.owner_badge, dec!(1))
            .call_method(self.collection,"set_auction_english", manifest_args!(initial, length))
            .call_method(actor.2, "deposit_batch", manifest_args!(ManifestExpression::EntireWorktop))
            .build();
        let receipt = self.execute(transaction, actor);
        receipt.expect_commit_success();
    }
    
    fn mint_nft(&mut self, actor: &Actor) -> NonFungibleLocalId {
        let transaction = ManifestBuilder::new()
            .create_proof_from_account_by_amount(actor.2, self.owner_badge, dec!(1))
            .call_method(self.collection,"mint_nft", manifest_args!("name", "url1", "color,blue;type,image"))
            .call_method(actor.2, "deposit_batch", manifest_args!(ManifestExpression::EntireWorktop))
            .build();
        let receipt = self.execute(transaction, actor);
        //println!("{:?}\n", receipt);
        let result = receipt.expect_commit_success();
        //scrypto_decode(&result[2].as_vec()).unwrap()
        result.output(2)
    }

    fn buy_nft(&mut self, actor: &Actor, id: &NonFungibleLocalId, amount: Decimal, should_fail: bool) {
        let transaction = ManifestBuilder::new()
            .withdraw_from_account(actor.2, RADIX_TOKEN, amount)
            .take_from_worktop(RADIX_TOKEN, |builder, bucket| {
                builder.call_method(self.collection,"buy_nft",
                    manifest_args!(
                        id.clone(),
                        bucket
                    )
                )
            })
            .call_method(actor.2, "deposit_batch", manifest_args!(ManifestExpression::EntireWorktop))
            .build();
        let receipt = self.execute(transaction, actor);
        //println!("{:?}\n", receipt);
        if should_fail {
          receipt.expect_commit_failure();
        } else {
          receipt.expect_commit_success();
        }
    }
    
    fn bid_nft(&mut self, actor: &Actor, id: &NonFungibleLocalId, amount: Decimal, should_fail: bool) -> Option<ResourceAddress>{
        let transaction = ManifestBuilder::new()
            .withdraw_from_account(actor.2, RADIX_TOKEN, amount)
            .take_from_worktop(RADIX_TOKEN, |builder, bucket| {
                builder.call_method(self.collection,"bid_nft",
                    manifest_args!(
                        id.clone(),
                        bucket
                    )
                )
            })
            .call_method(actor.2, "deposit_batch", manifest_args!(ManifestExpression::EntireWorktop))
            .build();
        let receipt = self.execute(transaction, actor);
        //println!("{:?}\n", receipt);
        if should_fail {
          receipt.expect_commit_failure();
          return None;
        } else {
          return receipt.expect_commit_success().new_resource_addresses().first().cloned();
        }
    }
    fn withdraw(&mut self, actor: &Actor, badge: ResourceAddress) {
        let transaction = ManifestBuilder::new()
            .withdraw_from_account(actor.2, badge, dec!(1))
            .take_from_worktop(badge, |builder, bucket| {
                builder.call_method(self.collection,"withdraw",
                    manifest_args!(
                        bucket
                    )
                )
            })
            .call_method(actor.2, "deposit_batch", manifest_args!(ManifestExpression::EntireWorktop))
            .build();
        let receipt = self.execute(transaction, actor);
        receipt.expect_commit_success();
    }
    
    fn close_auction(&mut self, actor: &Actor) {
        let transaction = ManifestBuilder::new()
            .call_method(self.collection,"close_auction", manifest_args!())
            .call_method(actor.2, "deposit_batch", manifest_args!(ManifestExpression::EntireWorktop))
            .build();
        let receipt = self.execute(transaction, actor);
        receipt.expect_commit_success();
    }
    
    fn collect_payments(&mut self, actor: &Actor) {
        let transaction = ManifestBuilder::new()
            .create_proof_from_account_by_amount(actor.2, self.owner_badge, dec!(1))
            .call_method(self.collection,"collect_payments", manifest_args!())
            .call_method(actor.2, "deposit_batch", manifest_args!(ManifestExpression::EntireWorktop))
            .build();
        let receipt = self.execute(transaction, actor);
        receipt.expect_commit_success();
    }
    
    fn list_present_nft(&mut self, actor: &Actor) {
        let transaction = ManifestBuilder::new()
            .call_method(self.collection,"list_present_nft", manifest_args!())
            .build();
        let receipt = self.execute(transaction, actor);
        receipt.expect_commit_success();
    }
    
    fn set_epoch(&mut self, epoch: u64) {
        self.runner.set_current_epoch(epoch);
    }
}

#[test]
fn test_creation() {
    let (mut env, owner, _) = TestEnv::new();
    env.mint_nft(&owner);
    env.set_fixed_auction(&owner, dec!(10));
}

#[test]
fn test_withdraw() {
    let (mut env, owner, _) = TestEnv::new();
    env.collect_payments(&owner);
}

#[test]
fn test_buy() {
    let (mut env, owner, buyers) = TestEnv::new();
    let id = env.mint_nft(&owner);
    env.set_fixed_auction(&owner, dec!(10));
    
    env.buy_nft(&buyers[0], &id, dec!(100), false);
    
    env.collect_payments(&owner);
}

#[test]
fn test_buy_fail() {
    let (mut env, owner, buyers) = TestEnv::new();
    let id = env.mint_nft(&owner);
    env.set_fixed_auction(&owner, dec!(10));
    
    env.buy_nft(&buyers[0], &id,  dec!(1), true);
    
    env.collect_payments(&owner);
}

#[test]
fn test_list_present() {
    let (mut env, owner, buyers) = TestEnv::new();
    let id = env.mint_nft(&owner);
    let id2 = env.mint_nft(&owner);
    env.set_fixed_auction(&owner, dec!(10));
    
    env.buy_nft(&buyers[0], &id,  dec!(10), false);
    
    env.list_present_nft(&buyers[0]);
}

#[test]
fn test_creation_dutch() {
    let (mut env, owner, buyers) = TestEnv::new();
    env.mint_nft(&owner);
    env.set_epoch(0);
    env.set_dutch_auction(&owner, dec!(11), dec!(1), 10);
}

#[test]
fn test_dutch_buy_beginning() {
    let (mut env, owner, buyers) = TestEnv::new();
    let id = env.mint_nft(&owner);
    env.set_epoch(0);
    env.set_dutch_auction(&owner, dec!(11), dec!(1), 10);
    env.set_epoch(0);
    env.buy_nft(&buyers[0], &id,  dec!(10), true);
    env.buy_nft(&buyers[0], &id,  dec!(11), false);
}

#[test]
fn test_dutch_buy_end() {
    let (mut env, owner, buyers) = TestEnv::new();
    let id = env.mint_nft(&owner);
    env.set_epoch(0);
    env.set_dutch_auction(&owner, dec!(11), dec!(1), 10);
    env.set_epoch(25);
    env.buy_nft(&buyers[0], &id,  dec!("0.99"), true);
    env.buy_nft(&buyers[0], &id,  dec!(1), false);
}

#[test]
fn test_dutch_buy_middle() {
    let (mut env, owner, buyers) = TestEnv::new();
    let id = env.mint_nft(&owner);
    env.set_epoch(0);
    env.set_dutch_auction(&owner, dec!(11), dec!(1), 10);
    env.set_epoch(5);
    env.buy_nft(&buyers[0], &id,  dec!(5), true);
    env.buy_nft(&buyers[0], &id,  dec!(6), false);
}

#[test]
fn test_creation_english() {
    let (mut env, owner, buyers) = TestEnv::new();
    env.mint_nft(&owner);
    env.set_epoch(0);
    env.set_english_auction(&owner, dec!(10), 10);
}

#[test]
fn test_close_english() {
    let (mut env, owner, buyers) = TestEnv::new();
    env.mint_nft(&owner);
    env.set_epoch(0);
    env.set_english_auction(&owner, dec!(10), 10);
    env.set_epoch(11);
    env.close_auction(&owner);
    env.collect_payments(&owner);
}

#[test]
fn test_single_bid() {
    let (mut env, owner, buyers) = TestEnv::new();
    let id = env.mint_nft(&owner);
    env.set_epoch(0);
    env.set_english_auction(&owner, dec!(10), 10);
    let badge = env.bid_nft(&buyers[0], &id, dec!(10), false).unwrap();
    env.set_epoch(11);
    env.close_auction(&owner);
    env.collect_payments(&owner);
    env.withdraw(&buyers[0], badge);
}

#[test]
fn test_bid_below_min() {
    let (mut env, owner, buyers) = TestEnv::new();
    let id = env.mint_nft(&owner);
    env.set_epoch(0);
    env.set_english_auction(&owner, dec!(10), 10);
    env.bid_nft(&buyers[0], &id, dec!(9), true);
    env.set_epoch(11);
    env.close_auction(&owner);
    env.collect_payments(&owner);
}

#[test]
fn test_bid_below_current() {
    let (mut env, owner, buyers) = TestEnv::new();
    let id = env.mint_nft(&owner);
    env.set_epoch(0);
    env.set_english_auction(&owner, dec!(10), 10);
    env.bid_nft(&buyers[0], &id, dec!(15), false);
    env.bid_nft(&buyers[0], &id, dec!(12), true);
    env.set_epoch(11);
    env.close_auction(&owner);
    env.collect_payments(&owner);
}