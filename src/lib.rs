use scrypto::prelude::*;
use std::cmp;

#[derive(ScryptoSbor, PartialEq)]
enum AuctionType {
    None,
    Fixed(Decimal),
    Dutch(Decimal, Decimal, Epoch, u64)
}

#[derive(ScryptoSbor, NonFungibleData)]
pub struct NFT {
    name: String,
    key_image_url: String,
    metadata: String
}

#[derive(ScryptoSbor, PartialEq)]
enum Status {
    NOTSTARTED,
    ONGOING,
    CLOSED
}

fn create_admin_badge(addr: ComponentAddress) -> FungibleBucket {
  ResourceBuilder::new_fungible(OwnerRole::None)
    .metadata(metadata! {
      init {
        "name" => "Impahla - collection owner badge", locked;
        "description" => "This token is the access badge used by owner to modify an existing collection in the ledger", locked;
        "symbol" => "IMP BADGE", locked;
        "component" => addr, locked;
        "icon_url" => Url::of("https://www.impahla.io/favicon.png"), locked;
        "info_url" => Url::of("https://www.impahla.io/"), locked;
      }
    })
    .divisibility(DIVISIBILITY_NONE)
    .burn_roles(burn_roles! {
      burner => rule!(allow_all);
      burner_updater => rule!(deny_all);
    })
    .mint_initial_supply(1)
}

#[blueprint]
mod nft_project {
    enable_method_auth! {
      methods {
          set_auction_fixed => restrict_to: [OWNER];
          set_auction_dutch => restrict_to: [OWNER];
          set_whitelist => restrict_to: [OWNER];
          mint_nft => restrict_to: [OWNER];
          start_auction => restrict_to: [OWNER];
          close_auction => restrict_to: [OWNER];
          collect_payments => restrict_to: [OWNER];
          buy_nft => PUBLIC;
      }
    }
  
    struct NftProject {
        // Status
        status: Status,
        /// Vault for our nfts
        nft_vault: NonFungibleVault,
        // Address of the collection
        nft_addr: ResourceAddress,
        /// Availability and metadata for one nft
        nft_available: HashMap<NonFungibleLocalId, bool>,
        /// Resource Manager
        resource_manager: ResourceManager,
        /// Vault that store all payments
        ccy_vault: FungibleVault,
        /// Currency address
        ccy_addr: ResourceAddress,
        /// Auction type
        auction_type: AuctionType,
        // Owner badge address
        owner_badge_address: ResourceAddress,
        /// Amount available to collect
        amount_to_collect: Decimal,
        // Whitelist NFT Address
        whitelist_address: Option<ResourceAddress>,
        // Whitelist max
        whitelist_max: Option<u16>,
        // Whitelist counter
        whitelist_counter: HashMap<NonFungibleLocalId, u16>,
    }

    impl NftProject {
        pub fn instantiate_component(ccy_addr: ResourceAddress, collection_name: String) -> (Global<NftProject>, FungibleBucket) {
            let (address_reservation, component_address) =
                Runtime::allocate_component_address(NftProject::blueprint_id()); 
            let owner_badge = create_admin_badge(component_address);
            let resource_manager = ResourceBuilder::new_ruid_non_fungible::<NFT>(
                                    OwnerRole::Updatable(rule!(require(owner_badge.resource_address()))))
                .metadata(metadata! { init { "name" => collection_name, locked; }} )
                .mint_roles(mint_roles! (
                    minter => rule!(require(global_caller(component_address)) || require(owner_badge.resource_address())); 
                    minter_updater => rule!(require(owner_badge.resource_address()));
                ))
                .create_with_no_initial_supply();
            let nft_addr = resource_manager.address();
            let component = Self {
                    status: Status::NOTSTARTED,
                    nft_vault: NonFungibleVault::new(nft_addr),
                    nft_addr: nft_addr,
                    nft_available: HashMap::new(),
                    resource_manager: resource_manager,
                    ccy_vault: FungibleVault::new(ccy_addr),
                    ccy_addr: ccy_addr,
                    auction_type: AuctionType::None,
                    owner_badge_address: owner_badge.resource_address(),
                    amount_to_collect: dec!(0),
                    whitelist_address: None,
                    whitelist_max: None,
                    whitelist_counter: HashMap::new()
                }.instantiate();
            let prepared_comp = 
              component.prepare_to_globalize(OwnerRole::Fixed(rule!(require(owner_badge.resource_address()))))
                       .with_address(address_reservation);
            (prepared_comp.globalize(), owner_badge)
        }
        
        pub fn set_auction_fixed(&mut self, cost: Decimal) {
            assert!(self.status == Status::NOTSTARTED, "cannot change auction type after auction has been started");
            self.auction_type = AuctionType::Fixed(cost);
        }
        
        pub fn set_auction_dutch(&mut self, initial_cost: Decimal, cost_decrease: Decimal, length: u64) {
            assert!(self.status == Status::NOTSTARTED, "cannot change auction type after auction has been started");
            self.auction_type = AuctionType::Dutch(initial_cost, cost_decrease, Runtime::current_epoch(), length);
        }
        
        pub fn set_whitelist(&mut self, address: ResourceAddress, max: u16) {
            assert!(self.status == Status::NOTSTARTED, "cannot change whitelist after auction has been started");
            self.whitelist_address = Some(address);
            self.whitelist_max = Some(max);
        }

        pub fn start_auction(&mut self) {
            assert!(self.status == Status::NOTSTARTED, "auction has been started already");
            assert!(self.auction_type != AuctionType::None, "cannot start an auction if the auction type is not defined");
            self.status = Status::ONGOING;
        }
        
        pub fn close_auction(&mut self) -> Vec<Bucket> {
            assert!(self.status == Status::ONGOING, "can't close an auction which is not ongoing");
            self.status = Status::CLOSED;
            let mut ret = Vec::<Bucket>::new();
            ret.push(self.nft_vault.take_all().into());
            ret.push(self.ccy_vault.take_all().into());
            self.amount_to_collect = dec!(0);
            for (_, available) in self.nft_available.iter_mut() {
              *available = false;
            }
            return ret;
        }
        
        pub fn buy_nft(&mut self, id: NonFungibleLocalId, mut payment: FungibleBucket, badge: Option<NonFungibleBucket>) -> Vec<Bucket> {
            assert!(self.status == Status::ONGOING, "can't buy from an auction which is not ongoing");
            let mut ret = Vec::<Bucket>::new();
            
            // Do the whitelist logic if needed
            match self.whitelist_address {
              Option::Some(address) => {
                let badge_bucket = badge.expect("the auction is using a whitelist, we expect a badge to be presented");
                assert!(badge_bucket.resource_address() == address, "the badge doesn't belong to the whitelist collection");
                let nft_id = badge_bucket.non_fungible_local_id();
                let counter = match self.whitelist_counter.get(&nft_id) {
                  Option::Some(counter) => counter+1,
                  Option::None => 1
                };
                self.whitelist_max.and_then(|max: u16| -> Option<u16> {
                  assert!(counter <= max, "this badge has already been used to buy all NFT it could");
                  None
                });
                self.whitelist_counter.insert(nft_id, counter);
                ret.push(badge_bucket.into());
              },
              Option::None => {}
            };
            
            // Deduce the current cost
            let current_cost = match self.auction_type {
              AuctionType::Fixed(cost) => cost,
              AuctionType::Dutch(initial, decrease, start, length) => {
                let diff = Runtime::current_epoch().number().checked_sub(start.number()).unwrap_or(0u64);
                let mut cost = initial - decrease * cmp::min(length, diff);
                if cost < Decimal::zero() {
                  cost = Decimal::zero();
                }
                cost
              },
              AuctionType::None => panic!("Auction not started")
            };
            
            // Take the requested NFT
            ret.push(self.nft_vault.take_non_fungible(&id).into());
            self.nft_available.insert(id, false);

            // Take our price out of the payment bucket
            self.ccy_vault.put(payment.take(current_cost));
            self.amount_to_collect = self.ccy_vault.amount();
            ret.push(payment.into());
            
            // Return the NFT and change
            return ret;
        }
        
        pub fn mint_nft(&mut self, name: String, url: String, metadata: String) -> NonFungibleLocalId {
            let new_nft = NFT {
                name: name,
                key_image_url: url,
                metadata: metadata.clone(),
            };
            let nft_bucket = self.resource_manager.mint_ruid_non_fungible(new_nft).as_non_fungible();
            let nft_id = nft_bucket.non_fungible_local_id();
            self.nft_available.insert(nft_id.clone(), true);
            self.nft_vault.put(nft_bucket);
            nft_id
        }
        /*
        pub fn add_nft(&mut self, nft_bucket: NonFungibleBucket) {
            assert!(self.status == Status::NOTSTARTED, "can't add an NFT to an auction which is not ongoing");
            for nft_id in nft_bucket.non_fungible_local_ids() {
              self.nft_available.insert(nft_id.clone(), true);
            }
            self.nft_vault.put(nft_bucket);
        }
        */
        pub fn collect_payments(&mut self) -> FungibleBucket {
            self.amount_to_collect = dec!(0);
            self.ccy_vault.take_all()
        }
    }
}
