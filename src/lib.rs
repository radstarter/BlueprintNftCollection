use scrypto::prelude::*;
use std::cmp;

#[derive(ScryptoSbor)]
enum AuctionType {
    None,
    Fixed(Decimal),
    Dutch(Decimal, Decimal, u64, u64),
    English(Decimal, u64, u64)
}

#[derive(ScryptoSbor, NonFungibleData)]
pub struct NFT {
    name: String,
    description: String,
    key_image_url: String,
    creator_url: String,
    ipfs_id: String,
    metadata: String
}

#[derive(ScryptoSbor, Clone)]
pub struct NFTInfo {
    available: bool,
    metadata: String,
}

fn create_owner_badge() -> Bucket {
  ResourceBuilder::new_fungible()
    .metadata("name", "Impahla - collection owner badge")
    .metadata("description", "This token is the access badge used by owner to modify an existing collection in the ledger")
    .metadata("symbol", "OWNER")
    .metadata("icon_url", "https://impahla-dev.netlify.app/img/icon.png")
    .metadata("info_url", "https://impahla-dev.netlify.app/")
    .divisibility(DIVISIBILITY_NONE)
    .burnable(rule!(allow_all), LOCKED)
    .mint_initial_supply(1)
}

fn create_bidder_badge() -> Bucket {
  ResourceBuilder::new_fungible()
    .metadata("name", "Impahla - bidder badge")
    .metadata("description", "This token is the badge used to identify the bidder")
    .metadata("symbol", "BIDDER")
    .metadata("icon_url", "https://impahla-dev.netlify.app/img/icon.png")
    .metadata("info_url", "https://impahla-dev.netlify.app/")
    .divisibility(DIVISIBILITY_NONE)
    .burnable(rule!(allow_all), LOCKED)
    .mint_initial_supply(1)
}

fn create_mint_badge() -> Bucket {
    ResourceBuilder::new_fungible()
        .divisibility(DIVISIBILITY_NONE)
        .burnable(rule!(allow_all), LOCKED)
        .mint_initial_supply(1)
}

type NFTInfosStore = ::std::collections::HashMap<NonFungibleLocalId, NFTInfo>;

#[blueprint]
mod nft_project {
    struct NftProject {
        /// Vault for our nfts
        nft_vault: Vault,
        /// The price for one nft
        nft_infos_store: NFTInfosStore,
        /// A vault that holds the mint badge
        nft_mint_badge: Vault,
        /// A vault that collects all XRD payments
        collected_xrd: Vault,
        /// Auction type
        auction_type: AuctionType,
        /// Current offer in an english auction
        current_offers: HashMap<NonFungibleLocalId, (ResourceAddress, Vault, Decimal)>,
        /// To be withdraw, failed and successful bids in an english auction (Token, Ccy)
        to_be_withdraw: HashMap<ResourceAddress, (Vault, Vault, Decimal, Decimal)>,
        // Owner badge address
        owner_badge_address: ResourceAddress,
        /// Amount available to collect
        amount_to_collect: Decimal,
        /// Dead vault
        dead_vault: Vec<Vault>
    }

    impl NftProject {
        pub fn instantiate_component() -> (ComponentAddress, Bucket) {
            let owner_badge = create_owner_badge();
            let nft_mint_badge = create_mint_badge();
            let nft_resource_def = ResourceBuilder::new_uuid_non_fungible::<NFT>()
                .metadata("name", "NFT Collection")
                .mintable(rule!(require(nft_mint_badge.resource_address())), LOCKED)
                .create_with_no_initial_supply();
            let component = Self {
                    nft_vault: Vault::new(nft_resource_def),
                    nft_infos_store: NFTInfosStore::new(),
                    nft_mint_badge: Vault::with_bucket(nft_mint_badge),
                    collected_xrd: Vault::new(RADIX_TOKEN),
                    auction_type: AuctionType::None,
                    current_offers: HashMap::new(),
                    to_be_withdraw: HashMap::new(),
                    owner_badge_address: owner_badge.resource_address(),
                    amount_to_collect: dec!(0),
                    dead_vault: Vec::new()
                }.instantiate();
                
            let mut rules = AccessRulesConfig::new();
            rules = rules.method("set_auction_fixed", rule!(require(owner_badge.resource_address())), AccessRule::DenyAll);
            rules = rules.method("set_auction_dutch", rule!(require(owner_badge.resource_address())), AccessRule::DenyAll);
            rules = rules.method("set_auction_english", rule!(require(owner_badge.resource_address())), AccessRule::DenyAll);
            rules = rules.method("mint_nft", rule!(require(owner_badge.resource_address())), AccessRule::DenyAll);
            rules = rules.method("collect_payments", rule!(require(owner_badge.resource_address())), AccessRule::DenyAll);
            rules = rules.default(rule!(allow_all), AccessRule::DenyAll);
            
            (component.globalize_with_access_rules(rules), owner_badge)
        }
        
        pub fn set_auction_fixed(&mut self, cost: Decimal) {
            self.auction_type = AuctionType::Fixed(cost);
        }
        
        pub fn set_auction_dutch(&mut self, initial_cost: Decimal, cost_decrease: Decimal, length: u64) {
            self.auction_type = AuctionType::Dutch(initial_cost, cost_decrease, Runtime::current_epoch(), length);
        }
        
        pub fn set_auction_english(&mut self, start_cost: Decimal, length: u64) {
            self.auction_type = AuctionType::English(start_cost, Runtime::current_epoch(), length);
        }
        
        pub fn bid_nft(&mut self, id: NonFungibleLocalId, payment: Bucket) -> Bucket {
            //assert!(end_of_auction <= Runtime::current_epoch(), "too late");
            assert!(
                self.collected_xrd.resource_address() == payment.resource_address(),
                "You try to pay in an incorrect token"
            );
            let start_cost = match self.auction_type {
                AuctionType::English(cost, _, _) => cost,
                _ => panic!("only available for english auction")
            };
            assert!(
                payment.amount() >= start_cost,
                "You try to pay in an incorrect token"
            );
            // create a badge to identify the bidder
            let badge = create_bidder_badge();
            let new_offer = payment.amount();
            // retreive the current offer on this NFT
            let current_offer = self.current_offers.remove(&id);
            if !current_offer.is_none() {
              // if there is one, retrieve the information
              let (address, vault, cost) = current_offer.unwrap();
              if cost >= new_offer {
                // return an error if the amount send by the new bidder doesn't beat the previous one
                panic!("another high offer already exist")
              }
              let nft_addr = self.nft_vault.resource_address();
              let amnt = vault.amount();
              // store the money of the previous winning bid, it may now be retrieve by the previous bidder
              self.to_be_withdraw.insert(address, (Vault::new(nft_addr), vault, dec!(0), amnt));
            };
            // insert the new bid for this NFT
            self.current_offers.insert(id, (badge.resource_address(), Vault::with_bucket(payment), new_offer));
            // return the badge of the bidder
            badge
        }

        pub fn close_auction(&mut self) {
            let end_of_auction = match self.auction_type {
                AuctionType::English(_, start, length) => start+length,
                _ => panic!("only available for english auction")
            };
            assert!(end_of_auction < Runtime::current_epoch(), "not yet time to resolve it");
            // Flatten the map nft => offer
            let mut offers: Vec<(NonFungibleLocalId, ResourceAddress, Vault)> = self
                .current_offers
                .drain()
                .map(|x| {
                    (
                        x.0,             // nft id
                        x.1 .0,          // badge addr
                        x.1 .1           // vault ccy
                    )
                })
                .collect();
            // use the flatten array to process each offer
            offers.drain(..).for_each(|(nft_id, badge_addr, mut vault_ccy)| {
              let ccy_addr = self.collected_xrd.resource_address();
              // create a vault with the requested nft
              let solo_nft_vault = Vault::with_bucket(self.nft_vault.take_non_fungible(&nft_id));
              // Mark it as sold
              self.nft_infos_store.get_mut(&nft_id).unwrap().available = false;
              // store it to be retreive later by the bidder
              self.to_be_withdraw.insert(badge_addr, (solo_nft_vault,Vault::new(ccy_addr), dec!(1), dec!(0)));
              // store the cost in the vault for the owner to retreive
              self.collected_xrd.put(vault_ccy.take_all());
              self.amount_to_collect = self.collected_xrd.amount();
              self.dead_vault.push(vault_ccy);
            });
            // convert the timed auction in a fixed auction for whatever is left
            let cost = match self.auction_type {
                AuctionType::English(initial, _, _) => initial,
                AuctionType::Dutch(initial, decrease, _, length) => initial - decrease * length,
                _ => panic!("not available for this kind of auction")
            };
            self.auction_type = AuctionType::Fixed(cost);
        }

        pub fn withdraw(&mut self, badge: Bucket) -> (Bucket, Bucket) {
            assert!(badge.amount().is_positive(), "The badge is empty");
            let vaults = self.to_be_withdraw.remove(&badge.resource_address())
                             .expect("no tokens available to withdraw");
            let (mut token_vault, mut ccy_vault, _, _) = vaults;
            let ret = (token_vault.take_all(), ccy_vault.take_all());
            self.dead_vault.push(token_vault);
            self.dead_vault.push(ccy_vault);
            badge.burn();
            return ret;
        }
        
        pub fn buy_nft(&mut self, id: NonFungibleLocalId, mut payment: Bucket) -> (Bucket, Bucket) {
            let current_cost = match self.auction_type {
              AuctionType::Fixed(cost) => cost,
              AuctionType::Dutch(initial, decrease, start, length) => {
                let mut cost = initial - decrease * cmp::min(length, Runtime::current_epoch() - start);
                if cost < Decimal::zero() {
                  cost = Decimal::zero();
                }
                cost
              },
              AuctionType::English(_initial, _start, _length) => panic!("For english auction, the user need to bid, not buy"),
              AuctionType::None => panic!("Auction not started")
            };
            
            // Take our price out of the payment bucket
            self.nft_infos_store.get_mut(&id).unwrap().available = false;
            self.collected_xrd.put(payment.take(current_cost));
            self.amount_to_collect = self.collected_xrd.amount();

            // Take the requested NFT
            let nft_bucket = self.nft_vault.take_non_fungible(&id);
            // Return the NFT and change
            (nft_bucket, payment)
        }        

        // Metadata are given as string with key1,value1;key2,value2;....
        pub fn mint_nft(&mut self, name: String, url: String, metadata: String) -> NonFungibleLocalId {
            //let owner_proof = ComponentAuthZone::create_proof(self.owner_badge_address);
            let new_nft = NFT {
                name: name,
                description: "".to_string(), // TODO
                key_image_url: url,
                creator_url: "".to_string(), // TODO
                ipfs_id: "".to_string(), // TODO
                metadata: metadata.clone(),
            };
            let nft_info = NFTInfo {
                available: true,
                metadata: metadata.clone(),
            };
            let (nft_bucket, nft_id) = self.nft_mint_badge.authorize(|| {
                let resource_manager = borrow_resource_manager!(self.nft_vault.resource_address());
                let bucket = resource_manager.mint_uuid_non_fungible(new_nft);
                let nft_id = bucket.non_fungible_local_id();
                (bucket, nft_id)
            });
            self.nft_infos_store.insert(nft_id.clone(), nft_info);
            self.nft_vault.put(nft_bucket);
            nft_id
        }

        pub fn collect_payments(&mut self) -> Bucket {
            self.amount_to_collect = dec!(0);
            self.collected_xrd.take_all()
        }

        pub fn list_present_nft(&mut self) -> NFTInfosStore {
            self.nft_infos_store.clone()
        }
    }
}
