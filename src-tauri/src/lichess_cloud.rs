use std::str::FromStr;

use pgn_reader::SanPlus;
use reqwest::Method;
use serde::{Deserialize, Serialize};
use shakmaty::{fen::Fen, uci::UciMove, CastlingMode, Chess, EnPassantMode, Position};
use vampirc_uci::uci::{Score, ScoreValue};

use crate::{chess::BestMoves, error::Error};

#[derive(Serialize, Deserialize, Debug)]
struct LichessCloudData {
    fen: String,
    knodes: u32,
    depth: u32,
    pvs: Vec<Pv>,
}

#[derive(Serialize, Deserialize, Debug)]
#[serde(untagged)]
enum Pv {
    Mate { mate: i32, moves: String },
    Cp { cp: i32, moves: String },
}

pub async fn get_cloud_best_moves(
    fen: &str,
    moves: &Vec<String>,
    multipv: usize,
) -> Result<Vec<BestMoves>, Error> {
    let mut lines: Vec<BestMoves> = Vec::with_capacity(5);

    let mut pos: Chess = Fen::from_ascii(fen.as_bytes())?.into_position(CastlingMode::Chess960)?;

    for m in moves {
        let uci = UciMove::from_ascii(m.as_bytes())?;
        let mv = uci.to_move(&pos)?;
        pos.play_unchecked(&mv);
    }

    let eval = get_cloud_evaluation(
        &Fen::from_position(pos.clone(), EnPassantMode::Legal).to_string(),
        multipv,
    )
    .await;

    if let Some(data) = eval {
        if data.pvs.len() < multipv {
            return Err(Error::NoMovesFound);
        }

        for pv in data.pvs.iter().take(multipv) {
            let mut pos_copy = pos.clone();

            let moves: Vec<String> = match pv {
                Pv::Cp { moves, .. } | Pv::Mate { moves, .. } => {
                    moves.split(' ').map(str::to_owned).collect()
                }
            };

            lines.push(BestMoves {
                // TODO: This should be multiplied by a factor of 1000?
                nodes: data.knodes,
                depth: data.depth,
                score: Score {
                    value: match pv {
                        Pv::Mate { mate, .. } => ScoreValue::Mate(*mate),
                        Pv::Cp { cp, .. } => ScoreValue::Cp(*cp),
                    },
                    wdl: None,
                    lower_bound: None,
                    upper_bound: None,
                },
                san_moves: moves
                    .iter()
                    .map(|m| {
                        let mov = UciMove::from_str(&m)?.to_move(&pos_copy)?;
                        Ok(SanPlus::from_move_and_play_unchecked(&mut pos_copy, &mov).to_string())
                    })
                    .collect::<Result<Vec<String>, Error>>()?,
                uci_moves: moves,
                multipv: multipv as u16,
                nps: 0,
            })
        }

        return Ok(lines);
    }

    Err(Error::NoMovesFound)
}

async fn get_cloud_evaluation(fen: &str, multipv: usize) -> Option<LichessCloudData> {
    let url = "https://lichess.org/api/cloud-eval";

    let client = reqwest::Client::new();
    let request = client
        .request(Method::GET, url)
        .query(&[("fen", fen), ("multiPv", &multipv.to_string())])
        .build()
        .unwrap();

    let result = client.execute(request).await;

    if let Ok(response) = result {
        if response.status() == 200 {
            return response.json::<LichessCloudData>().await.ok();
        }
    }

    None
}
