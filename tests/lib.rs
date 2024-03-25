use radix_engine::transaction::{TransactionReceipt};
use radix_engine::types::ManifestSbor;
use scrypto::prelude::*;
use scrypto_unit::*;
use transaction::builder::ManifestBuilder;
use transaction::model::{TransactionManifestV1};
use transaction::signing::secp256k1::Secp256k1PrivateKey;

type Actor = (Secp256k1PublicKey, Secp256k1PrivateKey, ComponentAddress);

struct TestEnv {
    runner: DefaultTestRunner,
    collection: ComponentAddress,
    owner_badge: ResourceAddress
}

#[derive(ScryptoSbor, NonFungibleData, ManifestSbor)]
struct EmptyNonFungibleData {}

fn create_non_fungible_tokens<'a>(
    runner: &mut DefaultTestRunner,
    owner: &Actor,
    ids: impl Iterator<Item = &'a u64>
) -> ResourceAddress {
    let mut entries = BTreeMap::new();
    ids.for_each(|i| -> () { entries.insert(NonFungibleLocalId::integer(*i), EmptyNonFungibleData {}); });

    let transaction = ManifestBuilder::new()
        .create_non_fungible_resource(OwnerRole::None, NonFungibleIdType::Integer, false, NonFungibleResourceRoles::default(), metadata!(), Some(entries))
        .deposit_batch(owner.2)
        .build();
    let receipt = runner.execute_manifest_ignoring_fee(transaction, vec![NonFungibleGlobalId::from_public_key(&owner.0)]);
    receipt.expect_commit_success();
    return receipt.expect_commit(true).new_resource_addresses()[0];
}

fn transfert_nft<'a>(
    runner: &mut DefaultTestRunner,
    addr: ResourceAddress,
    ids: impl Iterator<Item = &'a u64>,
    src: &Actor,
    dest: &Actor) {
    
    let mut entries = BTreeSet::new();
    ids.for_each(|i| -> () { entries.insert(NonFungibleLocalId::integer(*i)); });
    
    let transaction = ManifestBuilder::new()
        .withdraw_non_fungibles_from_account(src.2, addr, entries)
        .deposit_batch(dest.2)
        .build();
    let receipt = runner.execute_manifest_ignoring_fee(transaction, vec![
      NonFungibleGlobalId::from_public_key(&src.0),
      NonFungibleGlobalId::from_public_key(&dest.0)
    ]);
    receipt.expect_commit_success();
}

fn create_fungible_tokens(
    runner: &mut DefaultTestRunner,
    owner: &Actor,
    nb: Decimal
) -> ResourceAddress {
    let transaction = ManifestBuilder::new()
        .new_token_fixed(OwnerRole::None, metadata!(), nb)
        .deposit_batch(owner.2)
        .build();
    let receipt = runner.execute_manifest_ignoring_fee(transaction, vec![NonFungibleGlobalId::from_public_key(&owner.0)]);
    receipt.expect_commit_success();
    return receipt.expect_commit(true).new_resource_addresses()[0];
}

impl TestEnv {
    fn new(amount_new_ccy: Option<Decimal>) -> (
        TestEnv,
        Actor,      // seller: key, account
        Vec<Actor>, // buyers: key, account
        ResourceAddress
    ) {
        let mut runner = TestRunnerBuilder::new().without_trace().build();
        let seller = runner.new_allocated_account();
        
        let ccy_addr = match amount_new_ccy {
            Option::Some(amount) => create_fungible_tokens(&mut runner, &seller, amount),
            Option::None => XRD
        };
        
        let nft_ids = [1,2,3];
        let nft_addr = create_non_fungible_tokens(&mut runner, &seller, nft_ids.iter());
    
        let buyers: Vec<Actor> = (0..3).map(|_| runner.new_allocated_account()).collect();
        let package = runner.compile_and_publish(this_package!());
        
        let transaction = ManifestBuilder::new()
            .call_function(package, "NftProject", "instantiate_component", manifest_args!(ccy_addr, "NFT Collection"))
            .deposit_batch(seller.2)
            .build();
        let receipt = runner.execute_manifest_ignoring_fee(transaction, vec![NonFungibleGlobalId::from_public_key(&seller.0)]);
        //println!("{:?}\n", receipt);
        receipt.expect_commit_success();
        let result = &receipt.expect_commit(true);
        let collection = result.new_component_addresses()[0];
        let owner_badge = result.new_resource_addresses()[0];
        
        /*
        let mut entries = Vec::new();
        nft_ids.iter().for_each(|i| -> () { entries.push(NonFungibleLocalId::integer(*i)); });
            
        let transaction2 = ManifestBuilder::new()
            .create_proof_from_account_of_amount(seller.2, owner_badge, dec!(1))
            .call_method(collection,"add_nft", manifest_args!(entries))
            .deposit_batch(seller.2)
            .build();
        let receipt2 = runner.execute_manifest_ignoring_fee(transaction2, vec![NonFungibleGlobalId::from_public_key(&seller.0)]);
        println!("{:?}\n", receipt2);
        let result2 = receipt2.expect_commit_success();
        */
        (
            TestEnv {
                runner,
                owner_badge,
                collection,
            },
            seller,
            buyers,
            ccy_addr
        )
    }
    
    fn execute(&mut self, transaction: TransactionManifestV1, actor: &Actor) -> TransactionReceipt {
      self.runner.execute_manifest_ignoring_fee(transaction, vec![NonFungibleGlobalId::from_public_key(&actor.0)])
    }
    
    fn set_fixed_auction(&mut self, actor: &Actor, cost: Decimal){
        let transaction = ManifestBuilder::new()
            .create_proof_from_account_of_amount(actor.2, self.owner_badge, dec!(1))
            .call_method(self.collection,"set_auction_fixed", manifest_args!(cost))
            .deposit_batch(actor.2)
            .build();
        let receipt = self.execute(transaction, actor);
        receipt.expect_commit_success();
    }
    
    fn set_dutch_auction(&mut self, actor: &Actor, initial: Decimal, decrease: Decimal, length: u64){
        let transaction = ManifestBuilder::new()
            .create_proof_from_account_of_amount(actor.2, self.owner_badge, dec!(1))
            .call_method(self.collection,"set_auction_dutch", manifest_args!(initial, decrease, length))
            .deposit_batch(actor.2)
            .build();
        let receipt = self.execute(transaction, actor);
        receipt.expect_commit_success();
    }
    
    fn set_english_auction(&mut self, actor: &Actor, initial: Decimal, length: u64){
        let transaction = ManifestBuilder::new()
            .create_proof_from_account_of_amount(actor.2, self.owner_badge, dec!(1))
            .call_method(self.collection,"set_auction_english", manifest_args!(initial, length))
            .deposit_batch(actor.2)
            .build();
        let receipt = self.execute(transaction, actor);
        receipt.expect_commit_success();
    }
    
    fn set_whitelist(&mut self, actor: &Actor, addr: ResourceAddress, nb: u16){
        let transaction = ManifestBuilder::new()
            .create_proof_from_account_of_amount(actor.2, self.owner_badge, dec!(1))
            .call_method(self.collection,"set_whitelist", manifest_args!(addr, nb))
            .deposit_batch(actor.2)
            .build();
        let receipt = self.execute(transaction, actor);
        receipt.expect_commit_success();
    }
    
    fn mint_nft(&mut self, actor: &Actor) -> NonFungibleLocalId {
        let transaction = ManifestBuilder::new()
            .create_proof_from_account_of_amount(actor.2, self.owner_badge, dec!(1))
            .call_method(self.collection,"mint_nft", manifest_args!("name", "url1", "color,blue;type,image"))
            .deposit_batch(actor.2)
            .build();
        let receipt = self.execute(transaction, actor);
        //println!("{:?}\n", receipt);
        let result = receipt.expect_commit_success();
        //scrypto_decode(&result[2].as_vec()).unwrap()
        result.output(2)
    }
    
    fn buy_nft(&mut self, actor: &Actor, id_nft: &NonFungibleLocalId, amount: Decimal, badge: Option<&(ResourceAddress, NonFungibleLocalId)>, should_fail: bool) {
        let transaction = 
          match badge {
            Option::Some((address, id_whitelist)) => {
              let mut entries = BTreeSet::new();
              entries.insert(id_whitelist.clone());
              ManifestBuilder::new()
                .withdraw_from_account(actor.2, XRD, amount)
                .withdraw_non_fungibles_from_account(actor.2, *address, entries.clone())
                .take_all_from_worktop(XRD, "xrd")
                .take_non_fungibles_from_worktop(*address, entries, "nft")
                .call_method_with_name_lookup(self.collection,"buy_nft",
                  |lookup| (
                    id_nft.clone(),
                    lookup.bucket("xrd"),
                    Some(lookup.bucket("nft"))
                  )
                )
                .deposit_batch(actor.2)
                .build()
            },
            Option::None => {
              ManifestBuilder::new()
                .withdraw_from_account(actor.2, XRD, amount)
                .take_all_from_worktop(XRD, "xrd")
                .call_method_with_name_lookup(self.collection,"buy_nft",
                  |lookup| (
                    id_nft.clone(),
                    lookup.bucket("xrd"),
                    None::<ManifestBucket>
                  )
                )
                .deposit_batch(actor.2)
                .build()
            }
          };
        let receipt = self.execute(transaction, actor);
        println!("{:?}\n", receipt);
        if should_fail {
          receipt.expect_commit_failure();
        } else {
          receipt.expect_commit_success();
        }
    }
    
    fn withdraw(&mut self, actor: &Actor, badge: ResourceAddress) {
        let transaction = ManifestBuilder::new()
            .withdraw_from_account(actor.2, badge, dec!(1))
            .take_all_from_worktop(badge, "badge")
            .call_method_with_name_lookup(self.collection,"withdraw",
              |lookup| (
                lookup.bucket("badge"),
              )
            )
            .deposit_batch(actor.2)
            .build();
        let receipt = self.execute(transaction, actor);
        receipt.expect_commit_success();
    }
    
    fn start_auction(&mut self, actor: &Actor) {
        let transaction = ManifestBuilder::new()
            .create_proof_from_account_of_amount(actor.2, self.owner_badge, dec!(1))
            .call_method(self.collection,"start_auction", manifest_args!())
            .deposit_batch(actor.2)
            .build();
        let receipt = self.execute(transaction, actor);
        receipt.expect_commit_success();
    }
    
    fn close_auction(&mut self, actor: &Actor) {
        let transaction = ManifestBuilder::new()
            .create_proof_from_account_of_amount(actor.2, self.owner_badge, dec!(1))
            .call_method(self.collection,"close_auction", manifest_args!())
            .deposit_batch(actor.2)
            .build();
        let receipt = self.execute(transaction, actor);
        receipt.expect_commit_success();
    }
    
    fn collect_payments(&mut self, actor: &Actor) {
        let transaction = ManifestBuilder::new()
            .create_proof_from_account_of_amount(actor.2, self.owner_badge, dec!(1))
            .call_method(self.collection,"collect_payments", manifest_args!())
            .deposit_batch(actor.2)
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
        self.runner.set_current_epoch(Epoch::of(epoch));
    }
}

#[test]
fn test_creation() {
    let (mut env, owner, _, _) = TestEnv::new(None);
    env.mint_nft(&owner);
    env.set_fixed_auction(&owner, dec!(10));
    env.start_auction(&owner);
}

#[test]
fn test_withdraw() {
    let (mut env, owner, _, _) = TestEnv::new(None);
    env.collect_payments(&owner);
}

#[test]
fn test_buy() {
    let (mut env, owner, buyers, _) = TestEnv::new(None);
    let id = env.mint_nft(&owner);
    env.set_fixed_auction(&owner, dec!(10));
    env.start_auction(&owner);
    
    env.buy_nft(&buyers[0], &id, dec!(100), None, false);
    
    env.collect_payments(&owner);
}

#[test]
fn test_buy_wrong_ccy() {
    let (mut env, owner, buyers, _) = TestEnv::new(Some(dec!(1000)));
    let id = env.mint_nft(&owner);
    env.set_fixed_auction(&owner, dec!(10));
    env.start_auction(&owner);
    
    env.buy_nft(&buyers[0], &id, dec!(100), None, true);
    
    env.collect_payments(&owner);
}

#[test]
fn test_buy_whitelist_fail() {
    let (mut env, owner, buyers, _) = TestEnv::new(None);
    let id = env.mint_nft(&owner);
    env.set_fixed_auction(&owner, dec!(10));
    let addr = create_non_fungible_tokens(&mut env.runner, &owner, [1,2,3].iter());
    env.set_whitelist(&owner, addr, 1);
    env.start_auction(&owner);
    
    env.buy_nft(&buyers[0], &id, dec!(100), None, true);
    
    env.collect_payments(&owner);
}

#[test]
fn test_buy_whitelist_succeed() {
    let (mut env, owner, buyers, _) = TestEnv::new(None);
    let id = env.mint_nft(&owner);
    env.set_fixed_auction(&owner, dec!(10));
    let addr = create_non_fungible_tokens(&mut env.runner, &owner, [1,2,3].iter());
    transfert_nft(&mut env.runner, addr, [1].iter(), &owner, &buyers[0]);
    env.set_whitelist(&owner, addr, 1);
    env.start_auction(&owner);
    
    env.buy_nft(&buyers[0], &id, dec!(100), Some(&(addr, NonFungibleLocalId::integer(1))), false);
    
    env.collect_payments(&owner);
}

#[test]
fn test_buy_whitelist_buy_two_succeed() {
    let (mut env, owner, buyers, _) = TestEnv::new(None);
    let id = env.mint_nft(&owner);
    let id2 = env.mint_nft(&owner);
    env.set_fixed_auction(&owner, dec!(10));
    let addr = create_non_fungible_tokens(&mut env.runner, &owner, [1,2,3].iter());
    transfert_nft(&mut env.runner, addr, [1].iter(), &owner, &buyers[0]);
    env.set_whitelist(&owner, addr, 2);
    env.start_auction(&owner);
    
    env.buy_nft(&buyers[0], &id, dec!(100), Some(&(addr, NonFungibleLocalId::integer(1))), false);
    env.buy_nft(&buyers[0], &id2, dec!(100), Some(&(addr, NonFungibleLocalId::integer(1))), false);
    
    env.collect_payments(&owner);
}

#[test]
fn test_buy_whitelist_buy_two_fail() {
    let (mut env, owner, buyers, _) = TestEnv::new(None);
    let id = env.mint_nft(&owner);
    let id2 = env.mint_nft(&owner);
    env.set_fixed_auction(&owner, dec!(10));
    let addr = create_non_fungible_tokens(&mut env.runner, &owner, [1,2,3].iter());
    transfert_nft(&mut env.runner, addr, [1].iter(), &owner, &buyers[0]);
    env.set_whitelist(&owner, addr, 1);
    env.start_auction(&owner);
    
    env.buy_nft(&buyers[0], &id, dec!(100), Some(&(addr, NonFungibleLocalId::integer(1))), false);
    env.buy_nft(&buyers[0], &id2, dec!(100), Some(&(addr, NonFungibleLocalId::integer(1))), true);
    
    env.collect_payments(&owner);
}

#[test]
fn test_buy_fail() {
    let (mut env, owner, buyers, _) = TestEnv::new(None);
    let id = env.mint_nft(&owner);
    env.set_fixed_auction(&owner, dec!(10));
    env.start_auction(&owner);
    
    env.buy_nft(&buyers[0], &id,  dec!(1), None, true);
    
    env.collect_payments(&owner);
}

#[test]
fn test_creation_dutch() {
    let (mut env, owner, _, _) = TestEnv::new(None);
    env.mint_nft(&owner);
    env.set_epoch(0);
    env.set_dutch_auction(&owner, dec!(11), dec!(1), 10);
    env.start_auction(&owner);
}

#[test]
fn test_dutch_buy_beginning() {
    let (mut env, owner, buyers, _) = TestEnv::new(None);
    let id = env.mint_nft(&owner);
    env.set_epoch(0);
    env.set_dutch_auction(&owner, dec!(11), dec!(1), 10);
    env.start_auction(&owner);
    env.set_epoch(0);
    env.buy_nft(&buyers[0], &id,  dec!(10), None, true);
    env.buy_nft(&buyers[0], &id,  dec!(11), None, false);
}

#[test]
fn test_dutch_buy_end() {
    let (mut env, owner, buyers, _) = TestEnv::new(None);
    let id = env.mint_nft(&owner);
    env.set_epoch(0);
    env.set_dutch_auction(&owner, dec!(11), dec!(1), 10);
    env.start_auction(&owner);
    env.set_epoch(25);
    env.buy_nft(&buyers[0], &id,  dec!("0.99"), None, true);
    env.buy_nft(&buyers[0], &id,  dec!(1), None, false);
}

#[test]
fn test_dutch_buy_middle() {
    let (mut env, owner, buyers, _) = TestEnv::new(None);
    let id = env.mint_nft(&owner);
    env.set_epoch(0);
    env.set_dutch_auction(&owner, dec!(11), dec!(1), 10);
    env.start_auction(&owner);
    env.set_epoch(5);
    env.buy_nft(&buyers[0], &id,  dec!(5), None, true);
    env.buy_nft(&buyers[0], &id,  dec!(6), None, false);
}
