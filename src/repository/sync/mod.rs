use crate::repository::queue::Card;

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

pub async fn get_order(session: &Session<'_>) -> Result<Vec<Order>, anyhow::Error> {
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
    let stmt = match session.prepare(sql).await {
        Ok(s) => s,
        Err(e) => {
            error!("Failed to prepare statement for getting queue: {}", e);
            anyhow::bail!("Failed to prepare statement for getting queue: {}", e);
        }
    
    };
    let rows = match stmt.query(()).await {
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

