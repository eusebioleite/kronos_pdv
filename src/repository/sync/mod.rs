use sibyl::ToSql;

use crate::{config::{RootConfig, get_config}, repository::queue::Card};

pub struct Order {
    order_code: String,
    order_kind: String,
    schedule_code: u32,
    product_code: String,
    product_description: String,
    schedule_qtd: u32,
    product_bottle: u32,
    schedule_date: chrono::NaiveDate,
    order_company: u32,
    product_type: String,
    order_type: String,
    delivery_type: String,
    nature_code: u32,
    nature_description: String,
    order_seller: String,
    customer_code: String,
    customer_name: String,
    customer_fantasy: String,
    activity_type: u32,
}

impl Order {
    pub fn from_row(row: &Row) -> Result<Self, Error> {
        Ok(Self {
            order_code: row.get(0)?,
            order_kind: row.get(1)?,
            schedule_code: row.get(2)?,
            product_code: row.get(3)?,
            product_description: row.get(4)?,
            schedule_qtd: row.get(5)?,
            product_bottle: row.get(6)?,
            schedule_date: row.get(7)?,
            order_company: row.get(8)?,
            product_type: row.get(9)?,
            order_type: row.get(10)?,
            delivery_type: row.get(11)?,
            nature_code: row.get(12)?,
            nature_description: row.get(13)?,
            order_seller: row.get(14)?,
            customer_code: row.get(15)?,
            customer_name: row.get(16)?,
            customer_fantasy: row.get(17)?,
            activity_type: row.get(18)?,
        })
    }
}

pub struct NewCard {
    pub title: String,
    pub script: String,
    pub objective: String,
    pub activity_type: u32,
    pub kanban: u32,
    pub requester: u32,
    pub responsible: u32,
    pub planned_date: chrono::NaiveDate,
    pub replanned_date: chrono::NaiveDate,
    pub detail: Vec<OrderItem>,
}

pub fn get_detail(order: &Order) -> String {
    let mut sb = String::with_capacity(5000);
    sb.push_str(&format!("• <b>Produto:</b> {order.product_code} | {order.product_description}<br>\n"));
    sb.push_str("• <b>Quantidade:</b> ");

    if order.product_bottle == 0 {
        sb.push_str(&format!("{order.schedule_qtd} unidades"));
    } else {
        let boxes = order.schedule_qtd / order.product_bottle;
        let leftover = order.schedule_qtd % order.product_bottle;
        
        if boxes == 0 {
        sb.push_str(&format!("1 volume com {order.schedule_qtd} unidades no total volume incompleto)."));
        } else if leftover > 0 {
            sb.push_str(&format!("{boxes} volumes de {order.product_bottle} unidades ({order.schedule_qtd} unidades no total)."));
        } else {
            sb.push_str(&format!("{boxes} volumes de {order.product_bottle} unidades ({order.schedule_qtd} unidades no total)"));
        }
    }
    sb.push_str("<br><br>\n");
    sb
}

pub fn get_responsible(order: &Order) -> Result<u32, anyhow::Error> {
    let config = match get_config() {
        Ok(c) => c,
        Err(e) => return Err(anyhow::anyhow!("Failed to get config: {}", e)),
    };

    let company = match config.company.values().find(|c| c.code == order.order_company) {
        Some(c) => c,
        None => return Err(anyhow::anyhow!("Company {} not found", order.order_company)),
    };

    let column = match company.columns.values().find(|col| col.name.trim() == "PEDIDO EM CARTEIRA") {
        Some(col) => col,
        None => return Err(anyhow::anyhow!("Column 'PEDIDO EM CARTEIRA' not found for company {}", order.order_company)),
    };

    let responsible = config.get_responsible(company, column, order.product_type.trim());

    Ok(responsible)
}

pub fn get_kanban(order: &Order) -> Result<u32, anyhow::Error> {
    let config = match get_config() {
        Ok(c) => c,
        Err(e) => return Err(anyhow::anyhow!("Failed to get config: {}", e)),
    };
    
    let kanban = RootConfig::get_column_by_company(&config, order.order_company, "PEDIDO EM CARTEIRA");

    match kanban {
        Some(k) => Ok(k.code),
        None => Err(anyhow::anyhow!("Kanban {} not found", order.order_company)),
    }
}

pub fn get_requester(name: &str) -> Result<u32, anyhow::Error> {
    let config = match get_config() {
        Ok(c) => c,
        Err(e) => return Err(anyhow::anyhow!("Failed to get config: {}", e)),
    };

    let requester = RootConfig::get_requester_by_name(&config, name);

    match requester {
        Some(r) => Ok(r.code),
        None => Err(anyhow::anyhow!("Requester {} not found", name)),
    }
}

pub async fn get_order(session: &Session<'_>, card: &Card) -> Result<Vec<Order>, anyhow::Error> {
    
    let sql = "
    WITH itens_do_pedido AS (
        SELECT 
            prv_numped AS order_code,
            CASE
            WHEN pdv_tipped = 'A' THEN 'PDV-A'
            WHEN PDV_TIPPED = 'F' THEN 'PDV-F'
            END AS order_kind,
            prv_indice AS schedule_code,
            PRV_CODPRO AS product_code,
            PRO_DESCRI AS product_description,
            PRV_QTPROG AS schedule_qtd,
            PRO_QTDEMB AS product_bottle,
            prv_dtprog AS schedule_date,
            nvl(pdv_codseg, 100) AS order_company,
            CASE 
            WHEN PRO_DESCRI LIKE '%FRASCO%' THEN 'FRASCO' 
            ELSE 'PREFORMA' 
            END AS product_type,
            CASE 
            WHEN UPPER(NAT_DESCRI) LIKE '%AMOSTRA%' THEN 'AMOSTRA'
            WHEN UPPER(NAT_DESCRI) LIKE '%TRANSF%' THEN 'TRANSF'
            WHEN UPPER(NAT_DESCRI) LIKE '%VENDA%' THEN 'VENDA'
            WHEN UPPER(NAT_DESCRI) LIKE '%APONTAMENTO%' THEN 'VENDA' 
            ELSE 'OUTROS' 
            END AS order_type,
            CASE 
            WHEN PDV_TIPENT = 2 THEN 'ENTREGA'
            ELSE 'COLETA' 
            END AS delivery_type,
            pdv_indnat AS nature_code,
            nat_descri AS nature_description,
            pdv_vended AS order_seller,
            pdv_codemp AS customer_code,
            emp_erazao AS customer_name,
            emp_nfanta AS customer_fantasy
        FROM f_prgven
        JOIN f_pedvenda on prv_numped = pdv_numped
        LEFT JOIN f_cdemp on pdv_codemp = emp_codemp
        LEFT JOIN f_natope on pdv_indnat = nat_indice
        LEFT JOIN f_prods on prv_codpro = PRO_CODPRO
        WHERE PDV_TIPPED IN ('A', 'F')
    )
    SELECT 
    itens.*,
    CASE
        WHEN product_type = 'FRASCO' THEN
        CASE
            WHEN order_type = 'AMOSTRA' THEN 4
            WHEN order_type = 'TRANSF' THEN 7
            WHEN order_type = 'VENDA' THEN 2
            ELSE 2
        END
        WHEN product_type = 'PREFORMA' THEN
        CASE
            WHEN order_type = 'AMOSTRA' THEN 3
            WHEN order_type = 'TRANSF' THEN 6
            WHEN order_type = 'VENDA' THEN 1
            ELSE 1
        END
        ELSE 1
    END AS activity_type
    FROM itens_do_pedido itens
    ";
    sql.push_str("WHERE order_code = :1");

    let stmt = match session.prepare(sql).await {
        Ok(s) => s,
        Err(e) => {
            error!("Failed to prepare statement for getting queue: {}", e);
            anyhow::bail!("Failed to prepare statement for getting queue: {}", e);
        }
    
    };
    let rows = match stmt.query(&card.order_code).await {
        Ok(r) => r,
        Err(e) => {
            error!("Failed to execute query for getting queue: {}", e);
            anyhow::bail!("Failed to execute query for getting queue: {}", e);
        }
    };
    let mut orders = Vec::new();

    while let Some(row) = match rows.next().await {
        Ok(Some(r)) => Some(r),
        Ok(None) => None,
        Err(e) => {
            error!("Failed to fetch row: {}", e);
            anyhow::bail!("Failed to fetch row: {}", e);
        }
    } {
        let order = Order::from_row(&row)?;

        orders.push(order);
    }

    Ok(orders)
}

pub fn fill_card_data() {
    todo!();
}

pub fn order_to_card(order: &Order) -> Result<NewCard, anyhow::Error> {
    let type_activity = match order.product_type.as_str() {
        "FRASCO" => match order.order_type.as_str() {
            "AMOSTRA" => 4,
            "TRANSF" => 7,
            "VENDA" => 2,
            _ => 2,
        },
        "PREFORMA" => match order.order_type.as_str() {
            "AMOSTRA" => 3,
            "TRANSF" => 6,
            "VENDA" => 1,
            _ => 1,
        },
    };
    let title = format!("Pedido {order.order_code} - {order.customer_name}"); // titulo
    let planned_date; // data planejada
    let replanned_date; // data planejada
    let final_date; // data final
    let requester; // codigo vendedor crm
    let responsible; // codigo responsavel coluna
    let delivery_type; // tipo de frete
    let script; // fantasia cliente
    let detail; // detalhes do pedido
    let objective; // tipo_entrga
    let kanban; // codigo coluna
}

pub async fn process_order(session: &Session<'_>, card: &Card) -> Result<(), anyhow::Error> {
    let order = match get_order(&session, card).await {
        Ok(o) => o,
        Err(e) => {
            error!("Failed to get order: {}", e);
            anyhow::bail!("Failed to get order: {}", e);
        }
    };

    let new_cards = order_to_card(&order)
}