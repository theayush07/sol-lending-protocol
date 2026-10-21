use borsh::{BorshDeserialize, BorshSerialize};
use mock_oracle::{process_instruction, OracleInstruction, PriceFeedAccount, ORACLE_MAGIC};
use solana_program_test::*;
use solana_sdk::{
    instruction::{AccountMeta, Instruction},
    signature::{Keypair, Signer},
    system_instruction,
    transaction::Transaction,
};

#[tokio::test]
async fn test_oracle_init_and_update() {
    let program_id = mock_oracle::id();
    let pt = ProgramTest::new("mock_oracle", program_id, processor!(process_instruction));
    let (mut banks_client, payer, recent_blockhash) = pt.start().await;

    let feed_kp = Keypair::new();
    let rent = banks_client.get_rent().await.unwrap();
    let lamports = rent.minimum_balance(PriceFeedAccount::LEN);

    let create_ix = system_instruction::create_account(
        &payer.pubkey(),
        &feed_kp.pubkey(),
        lamports,
        PriceFeedAccount::LEN as u64,
        &program_id,
    );

    let init_ix = Instruction {
        program_id,
        accounts: vec![
            AccountMeta::new(feed_kp.pubkey(), false),
            AccountMeta::new_readonly(payer.pubkey(), true),
        ],
        data: borsh::to_vec(&OracleInstruction::Initialize {
            initial_price_micro: 150_000_000, // $150.00
            confidence_micro: 50_000,
        })
        .unwrap(),
    };

    let tx = Transaction::new_signed_with_payer(
        &[create_ix, init_ix],
        Some(&payer.pubkey()),
        &[&payer, &feed_kp],
        recent_blockhash,
    );
    banks_client.process_transaction(tx).await.unwrap();

    // Update price to $120.00
    let update_ix = Instruction {
        program_id,
        accounts: vec![
            AccountMeta::new(feed_kp.pubkey(), false),
            AccountMeta::new_readonly(payer.pubkey(), true),
        ],
        data: borsh::to_vec(&OracleInstruction::SetPrice {
            price_micro: 120_000_000,
            confidence_micro: 40_000,
        })
        .unwrap(),
    };

    let tx2 = Transaction::new_signed_with_payer(
        &[update_ix],
        Some(&payer.pubkey()),
        &[&payer],
        recent_blockhash,
    );
    banks_client.process_transaction(tx2).await.unwrap();

    let acct = banks_client.get_account(feed_kp.pubkey()).await.unwrap().unwrap();
    let decoded = PriceFeedAccount::try_from_slice(&acct.data[..PriceFeedAccount::LEN]).unwrap();
    assert_eq!(decoded.magic, ORACLE_MAGIC);
    assert_eq!(decoded.price_usd_micro, 120_000_000);
}