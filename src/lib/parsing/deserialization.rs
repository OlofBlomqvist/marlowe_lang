use std::collections::{HashSet, HashMap};

use pest::iterators::Pair;
use serde::de::value;
use crate::parsing::Rule;
use crate::types::marlowe::*;

struct Operation<'a> {
    pair_rule_type : Rule,
    thing_to_extract_ast_from : pest::iterators::Pairs<'a,Rule>,
    extracted_child_ast_nodes : Vec<AstNode>,
    string_representation : Option<String>
}

fn add_to_parent(call_stack:&mut Vec<Operation>,result_stack:&mut Vec<AstNode>,node:AstNode) {
    match call_stack.last_mut() {
        Some(parent) => {
            parent.extracted_child_ast_nodes.push(node)
        },
        None => { 
            result_stack.push(node)
        },
    }
}

fn option_to_result<T>(maybe_ast_node:Option<T>,msg:&str) -> Result<T,&str> {
    match maybe_ast_node {
        Some(x) => Ok(x),
        None => return Err(msg),
    }
}

fn parse_with_input<T>(pair:Pair<Rule>,input:HashMap<String,i64>) -> std::result::Result<T,String>
    where Result<T, String>: From<AstNode> { 
        parse_raw(pair,input)?.into() }

fn parse_raw(pair:Pair<Rule>,input:HashMap<String,i64>) -> Result<AstNode,String> {
    
    let mut keys : Vec<String> = input.keys().map(|x|x.to_owned()).collect();
    keys.dedup();

    if input.keys().len() != keys.len() {
        return Err(String::from("Input data cannot contain duplicate keys."))
    }

    let rule = pair.as_rule();
    let outer_inner = pair.clone().into_inner();
    let child_count = outer_inner.clone().count();
    let str_rep = if child_count == 0 { Some(pair.as_str().to_string()) } else { None };
    let mut call_stack : Vec<Operation> = vec![
        Operation{ 
            pair_rule_type: rule, 
            thing_to_extract_ast_from: outer_inner, 
            extracted_child_ast_nodes: vec![],
            string_representation: str_rep
        }];

    let mut result_stack : Vec<AstNode> = vec![];

    while let Some(mut current_operation) = call_stack.pop() {

        if let Some(child_pair) = current_operation.thing_to_extract_ast_from.next() {
            let rule = child_pair.as_rule();
            let inner = child_pair.clone().into_inner();
            let child_count = inner.clone().count();
            let str_rep = if child_count == 0 { Some(child_pair.as_str().to_string()) } else { None };
            let op = Operation {
                pair_rule_type: rule,
                thing_to_extract_ast_from: inner,
                extracted_child_ast_nodes: vec![],
                string_representation: str_rep
            };
            call_stack.push(current_operation);
            call_stack.push(op);
            continue;
        } 

        let get_next_node = |x:&mut Operation| {
            match x.extracted_child_ast_nodes.pop() {
                Some(v) => Ok(v),
                None => Err(format!("Failed to get next node from a {:?}. This is likely a bug in the marlowe_lang library",x.pair_rule_type)),
            }
        };

        macro_rules! fold_back {
            ($node:expr) => {
                add_to_parent(&mut call_stack, &mut result_stack, $node)
            };
        }

        macro_rules! try_get_next { () => { (get_next_node(&mut current_operation))};}
        macro_rules! get_next { () => { try_get_next!()? } }
        macro_rules! get_next_into { () => { get_next!().try_into()? };}
        match current_operation.pair_rule_type {  
            Rule::Contract => fold_back!(get_next!()),
            Rule::Close => fold_back!(AstNode::MarloweContract(Contract::Close)),
            Rule::UseValue => fold_back!(AstNode::MarloweValue(Value::UseValue(get_next_into!()))),            
            Rule::ConstantParam => {
                let parameter_name : String = get_next_into!();
                let input_parameter_value : Option<&i64> = input.get(&parameter_name);
                match input_parameter_value {
                    Some(value_from_input) => {
                        fold_back!(AstNode::MarloweValue(Value::ConstantValue(*value_from_input)))
                    },
                    None => {
                        fold_back!(AstNode::MarloweValue(Value::ConstantParam(parameter_name)))
                    },
                }
            },
            Rule::ArrayOfCases => fold_back!(AstNode::MarloweCaseList(current_operation.extracted_child_ast_nodes)),
            Rule::ArrayOfBounds => fold_back!(AstNode::MarloweBoundList(current_operation.extracted_child_ast_nodes)),
            Rule::PK => fold_back!(AstNode::MarloweParty(Party::PK { pk_hash : get_next_into!() })),
            Rule::TimeParam => {
                let parameter_name : String = get_next_into!();
                let input_parameter_value : Option<&i64> = input.get(&parameter_name);
                match input_parameter_value {
                    Some(value_from_input) => {
                        fold_back!(AstNode::MarloweTimeout(Timeout::TimeConstant(*value_from_input)))
                    },
                    None => fold_back!(AstNode::MarloweTimeout(Timeout::TimeParam(parameter_name))),
                }
            },
            Rule::PayeeAccount => fold_back!(AstNode::MarlowePayee(Payee::Account(get_next_into!()))),
            Rule::PayeeParty => fold_back!(AstNode::MarlowePayee(Payee::Party(get_next_into!()))),
            Rule::Role => fold_back!(AstNode::MarloweParty(Party::Role { role_token : get_next_node(&mut current_operation)?.try_into()?})),
            Rule::Notify => fold_back!(AstNode::MarloweAction(Action::Notify { 
                notify_if: get_next_node(&mut current_operation)?.try_into()? })),            
            Rule::Case => {
                let continuation_contract = get_next!();
                let contract_node = continuation_contract.try_into()?;
                let action = get_next_into!();
                fold_back!(AstNode::MarloweCase(crate::types::marlowe::Case {
                    case: action,
                    then: contract_node
                }));
            }
            Rule::When => {
                let contract_node = get_next_into!();
                let timeout = get_next_into!();
                let cases = get_next_into!();
                fold_back!(AstNode::MarloweContract(Contract::When { 
                    when: cases,
                    timeout: timeout, 
                    timeout_continuation: contract_node
                }))
            }
            Rule::ADA => fold_back!(AstNode::MarloweToken(Token::ADA)),
            Rule::Currency => {
                let v2 : String = get_next_into!();
                let v1 : String = get_next_into!();
                let token = Token::Custom { currency_symbol: v1, token_name: v2 };
                fold_back!(AstNode::MarloweToken(token))
            }
            Rule::Deposit => {
                let value = get_next_into!();
                let token = get_next_into!();
                let by_party = get_next_into!();
                let into_account_of = get_next_into!();
                fold_back!(AstNode::MarloweAction(Action::Deposit { 
                    party: by_party, 
                    into_account: into_account_of, 
                    of_token: token, 
                    deposits: value 
                }))
            }
            Rule::ChoiceId => {
                let party = get_next_into!();
                let s = get_next_into!();
                fold_back!(AstNode::MarloweChoiceId(ChoiceId {
                    choice_name: s, choice_owner: party,
                }))
            }
            Rule::Bound => {
                let (end,start) = (get_next_into!(),get_next_into!());
                fold_back!(AstNode::MarloweBound(Bound(start,end)))
            }
            Rule::Choice => {
                let bound = get_next_into!();
                let choice_id = get_next_into!();
                fold_back!(AstNode::MarloweAction( Action::Choice  { for_choice: choice_id, choose_between: bound } ))
            }
            Rule::string => {
                let s = option_to_result(current_operation.string_representation,"failed to parse a string.")?;
                fold_back!(AstNode::StringValue(s))
            }
            Rule::TrueObs => fold_back!(AstNode::MarloweObservation(Observation::True)),
            Rule::FalseObs => fold_back!(AstNode::MarloweObservation(Observation::False)),
            Rule::Number => {                
                let n = option_to_result(current_operation.string_representation,"Failed to parse a number!")?;
                let nn = match n.parse::<i64>() {
                    Ok(nnn) => nnn,
                    Err(e) => return Err(format!("{}. Inner error: {:?}","Failed to parse a number",e))
                };
                fold_back!(AstNode::MarloweNumber(nn))
            }
            Rule::Pay => {
                let continuation = get_next_into!();
                let value = get_next_into!();
                let token = get_next_into!();
                let payee = get_next_into!();
                let party = get_next_into!();
                fold_back!(AstNode::MarloweContract(Contract::Pay { 
                    from_account: party, 
                    to: payee, 
                    token: token, 
                    pay: value, 
                    then: continuation
                }))
            }
            Rule::SubValue => {
                let v2 = get_next_into!();
                let v1 = get_next_into!();
                fold_back!(AstNode::MarloweValue(Value::SubValue(v1,v2)))
            }
            Rule::MulValue => {
                let v2 = get_next_into!();
                let v1 = get_next_into!();
                fold_back!(AstNode::MarloweValue(Value::MulValue(v1,v2)))
            }
            Rule::ValueGT => {
                let v2 = get_next_into!();
                let v1 = get_next_into!();
                fold_back!(AstNode::MarloweObservation(Observation::ValueGT {
                    value: v1,
                    gt_than: v2
                }))
            }
            Rule::ValueGE => {
                let v2 = get_next_into!();
                let v1 = get_next_into!();
                fold_back!(AstNode::MarloweObservation(Observation::ValueGE {
                    value: v1,
                    ge_than: v2
                }))
            }
            Rule::ValueLT => {
                let v2 = get_next_into!();
                let v1 = get_next_into!();
                fold_back!(AstNode::MarloweObservation(Observation::ValueLT {
                    value: v1,
                    lt_than: v2
                }))
            }
            Rule::NegValue => {
                let v = get_next_into!();
                fold_back!(AstNode::MarloweValue(Value::NegValue(v)))
            }
            Rule::Cond => {
                let v2 = get_next_into!();
                let v1 = get_next_into!();
                let observation = get_next_into!();
                fold_back!(AstNode::MarloweValue(Value::Cond(observation, v1, v2)))
            }
            Rule::ChoiceValue => {
                let v = get_next_into!();
                fold_back!(AstNode::MarloweValue(Value::ChoiceValue(v)))
            }
            Rule::AvailableMoney => {
                let t = get_next_into!();
                let p = get_next_into!();
                fold_back!(AstNode::MarloweValue(Value::AvailableMoney(p,t)))
            }
            Rule::AddValue => {
                let v2 = get_next_into!();
                let v1 = get_next_into!();
                fold_back!(AstNode::MarloweValue(Value::AddValue(v1,v2)))
            }
            Rule::DivValue => {
                let v2 = get_next_into!();
                let v1 = get_next_into!();
                fold_back!(AstNode::MarloweValue(Value::DivValue(v1,v2)))
            }
            Rule::TimeIntervalEnd => {
                fold_back!(AstNode::MarloweValue(Value::TimeIntervalEnd))
            },
            Rule::TimeIntervalStart => {
                fold_back!(AstNode::MarloweValue(Value::TimeIntervalStart))
            }
            Rule::TimeConstant => {
                let vv = option_to_result(current_operation.string_representation,"failed to parse time constant")?;
                let vvv = match vv.parse::<i64>() {
                    Ok(n) => n,
                    Err(e) => return Err(format!("{} : {e:?}",format!("failed to convert time constant value {vv}' to i64."))),
                };
                fold_back!(AstNode::MarloweTimeout(Timeout::TimeConstant(vvv)))
            }
            Rule::If => {
                let else_contract = get_next_into!();
                let then_contract = get_next_into!();
                let observation = get_next_into!();
                fold_back!(AstNode::MarloweContract(Contract::If { 
                    r#if: observation, then: then_contract, r#else: else_contract 
                }))
            }
            Rule::Let => {
                let continue_as = get_next_into!();
                let value = get_next_into!();
                let s: Option<String> = get_next_into!();
                fold_back!(AstNode::MarloweContract(Contract::Let { 
                    r#let: option_to_result(s,"Failed to parse a 'let' contract node.")?, 
                    be: value, 
                    then: continue_as 
                }))
            }
            Rule::Constant => {
                let n = option_to_result(current_operation.extracted_child_ast_nodes.pop(),"failed to parse constant")?;
                let nn : i64 = n.try_into()?;
                fold_back!(AstNode::MarloweValue(Value::ConstantValue(nn)))

            }
            Rule::PubKey => {
                let v = option_to_result(current_operation.string_representation,"failed to parse pubkey")?;
                let vv = v.trim_start_matches("\"").trim_end_matches("\"");
                fold_back!(AstNode::StringValue(vv.to_string()));
            }
            
            Rule::ActionHole|
            Rule::ContractHole|
            Rule::TokenHole|
            Rule::CaseHole|
            Rule::ValueHole|
            Rule::ObservationHole|
            Rule::PartyHole|
            Rule::FromPartyHole|
            Rule::BoundHole|
            Rule::PayeeHole|
            Rule::TimeoutHole => fold_back!(AstNode::Null),
            
            unmatched_rule => {
                return Err(format!("The rule {unmatched_rule:?} is ready to be intialized but there is no match for it.. it has these children {:?}. This is most likely a bug in the marlowe_lang library. ",current_operation.extracted_child_ast_nodes))
            }
        }
         
    }
    if result_stack.len() != 1 {
        return Err(format!("Marlowe_Lang::ErrorCode(1) : {:?}",result_stack).to_string())
    };
    match result_stack.pop() {
        Some(v) => {
            Ok(v)
        }
        _ => Err("Marlowe_Lang::ErrorCode(2)".to_string())
    }
}



/// Parses a string into an instance of a Marlowe contract
pub fn deserialize(content:&str) -> Result<Contract,String>  { 
    deserialize_with_input(content,Default::default())
}
/// Parses a string into an instance of a Marlowe contract using the input data 
/// to populate constant and timeout parameters.
pub fn deserialize_with_input(content:&str,input:HashMap<String,i64>) -> Result<Contract,String>  {
    match <super::MarloweParser as pest::Parser::<Rule>>::parse(
        Rule::MainContract, 
        content
    ) {
        Result::Ok(mut pairs) => {
            match pairs.next() {
                None => Result::Err("it doesn't look like anything to me.".to_string()),
                Some(root) => {
                    match parse_with_input::<Contract>(root,input) {
                        Ok(v) => Ok(v),
                        Err(e) => Err(e),
                    }
                }
            }
        }
        Result::Err(e) => Err(format!("{e:#}"))
    }
}

