--drop table kronos_pdv_queue;

create table kronos_pdv_queue (
   order_code     varchar2(40) not null,
   schedule_code  number not null,
   status         varchar2(20) default 'NOVO' not null,
   retries        number default 0 not null,
   last_error     varchar2(4000),
   created_at     timestamp(6) default systimestamp not null,
   updated_at     timestamp(6) default systimestamp not null,
   constraint pk_kronos_pdv_queue primary key ( order_code, schedule_code )
);

create or replace trigger trg_kronos_pdv_queue_updated_at before
   update on kronos_pdv_queue
   for each row
begin
   :new.updated_at := systimestamp;
end;

comment on table kronos_pdv_queue is
   'Fila de integração do Kronos_PDV, automação responsável por transformar pedidos de venda do Debx em cards do DealerCRM.';

comment on column kronos_pdv_queue.order_code is
   'Código do pedido no Debx.';

comment on column kronos_pdv_queue.schedule_code is
   'Código da programação no Debx.';

comment on column kronos_pdv_queue.status is
   'Estado da sincronização: ATUALIZAR, NOVO, EXCLUIR, TRAVADO, SUCESSO, EXCLUIDO.';

comment on column kronos_pdv_queue.retries is
   'Contador de tentativas de sincronizar, máximo de 5.';

comment on column kronos_pdv_queue.last_error is
   'Ultimo erro de sincronização.';

comment on column kronos_pdv_queue.created_at is
   'Timestamp de criação.';

comment on column kronos_pdv_queue.updated_at is
   'Timestamp da última modificação.';

create index idx_kronos_pdv_queue_status on
   kronos_pdv_queue (
      status
   );
 
grant all privileges on inventario.kronos_pdv_queue to kronos;